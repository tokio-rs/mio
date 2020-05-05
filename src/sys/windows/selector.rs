use super::afd::{self, Afd, AfdPollInfo};
use super::io_status_block::IoStatusBlock;
use super::Event;
use crate::sys::Events;
use crate::Interest;

use miow::iocp::{CompletionPort, CompletionStatus};
use miow::Overlapped;
use std::collections::VecDeque;
use std::marker::PhantomPinned;
use std::os::windows::io::RawSocket;
use std::pin::Pin;
#[cfg(debug_assertions)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{io, ptr, thread};
use winapi::shared::ntdef::NT_SUCCESS;
use winapi::shared::ntdef::{HANDLE, PVOID};
use winapi::shared::ntstatus::STATUS_CANCELLED;
use winapi::shared::winerror::{ERROR_INVALID_HANDLE, ERROR_IO_PENDING, WAIT_TIMEOUT};
use winapi::um::handleapi::CloseHandle;
use winapi::um::minwinbase::OVERLAPPED;
use winapi::um::synchapi::{CreateEventA, SetEvent};

/// Overlapped value to indicate a `Waker` event.
//
// Note: this must be null, `SelectorInner::feed_events` depends on it.
pub const WAKER_OVERLAPPED: *mut Overlapped = ptr::null_mut();

#[derive(Debug)]
struct AfdGroup {
    cp: Arc<CompletionPort>,
    afd_group: Mutex<Vec<Arc<Afd>>>,
}

impl AfdGroup {
    pub fn new(cp: Arc<CompletionPort>) -> AfdGroup {
        AfdGroup {
            afd_group: Mutex::new(Vec::new()),
            cp,
        }
    }

    pub fn release_unused_afd(&self) {
        let mut afd_group = self.afd_group.lock().unwrap();
        afd_group.retain(|g| Arc::strong_count(&g) > 1);
    }
}

cfg_net! {
    const POLL_GROUP__MAX_GROUP_SIZE: usize = 32;

    impl AfdGroup {
        pub fn acquire(&self) -> io::Result<Arc<Afd>> {
            let mut afd_group = self.afd_group.lock().unwrap();
            if afd_group.len() == 0 {
                self._alloc_afd_group(&mut afd_group)?;
            } else {
                // + 1 reference in Vec
                if Arc::strong_count(afd_group.last().unwrap()) >= POLL_GROUP__MAX_GROUP_SIZE + 1 {
                    self._alloc_afd_group(&mut afd_group)?;
                }
            }

            match afd_group.last() {
                Some(arc) => Ok(arc.clone()),
                None => unreachable!(
                    "Cannot acquire afd, {:#?}, afd_group: {:#?}",
                    self, afd_group
                ),
            }
        }

        fn _alloc_afd_group(&self, afd_group: &mut Vec<Arc<Afd>>) -> io::Result<()> {
            let afd = Afd::new(&self.cp)?;
            let arc = Arc::new(afd);
            afd_group.push(arc);
            Ok(())
        }
    }
}

#[derive(Debug)]
enum SockPollStatus {
    Idle,
    Pending,
    Cancelled,
}

#[derive(Debug)]
pub struct AfdSockState {
    iosb: IoStatusBlock,
    poll_info: AfdPollInfo,
    afd: Arc<Afd>,

    raw_socket: RawSocket,
    base_socket: RawSocket,

    user_evts: u32,
    pending_evts: u32,

    user_data: u64,

    poll_status: SockPollStatus,
    delete_pending: bool,

    // last raw os error
    error: Option<i32>,

    pinned: PhantomPinned,
}

impl AfdSockState {
    fn update(&mut self, self_arc: &Pin<Arc<Mutex<SockState>>>) -> io::Result<()> {
        assert!(!self.delete_pending);

        // make sure to reset previous error before a new update
        self.error = None;

        if let SockPollStatus::Pending = self.poll_status {
            if (self.user_evts & afd::KNOWN_EVENTS & !self.pending_evts) == 0 {
                /* All the events the user is interested in are already being monitored by
                 * the pending poll operation. It might spuriously complete because of an
                 * event that we're no longer interested in; when that happens we'll submit
                 * a new poll operation with the updated event mask. */
            } else {
                /* A poll operation is already pending, but it's not monitoring for all the
                 * events that the user is interested in. Therefore, cancel the pending
                 * poll operation; when we receive it's completion package, a new poll
                 * operation will be submitted with the correct event mask. */
                if let Err(e) = self.cancel() {
                    self.error = e.raw_os_error();
                    return Err(e);
                }
                return Ok(());
            }
        } else if let SockPollStatus::Cancelled = self.poll_status {
            /* The poll operation has already been cancelled, we're still waiting for
             * it to return. For now, there's nothing that needs to be done. */
        } else if let SockPollStatus::Idle = self.poll_status {
            /* No poll operation is pending; start one. */
            self.poll_info.exclusive = 0;
            self.poll_info.number_of_handles = 1;
            *unsafe { self.poll_info.timeout.QuadPart_mut() } = std::i64::MAX;
            self.poll_info.handles[0].handle = self.base_socket as HANDLE;
            self.poll_info.handles[0].status = 0;
            self.poll_info.handles[0].events = self.user_evts | afd::POLL_LOCAL_CLOSE;

            // Increase the ref count as the memory will be used by the kernel.
            let overlapped_ptr = into_overlapped(self_arc.clone());

            let result = unsafe {
                self.afd
                    .poll(&mut self.poll_info, &mut *self.iosb, overlapped_ptr)
            };
            if let Err(e) = result {
                let code = e.raw_os_error().unwrap();
                if code == ERROR_IO_PENDING as i32 {
                    /* Overlapped poll operation in progress; this is expected. */
                } else {
                    // Since the operation failed it means the kernel won't be
                    // using the memory any more.
                    drop(from_overlapped(overlapped_ptr as *mut _));
                    if code == ERROR_INVALID_HANDLE as i32 {
                        /* Socket closed; it'll be dropped. */
                        self.mark_delete();
                        return Ok(());
                    } else {
                        self.error = e.raw_os_error();
                        return Err(e);
                    }
                }
            }

            self.poll_status = SockPollStatus::Pending;
            self.pending_evts = self.user_evts;
        } else {
            unreachable!("Invalid poll status during update, {:#?}", self)
        }

        Ok(())
    }

    fn cancel(&mut self) -> io::Result<()> {
        match self.poll_status {
            SockPollStatus::Pending => {}
            _ => unreachable!("Invalid poll status during cancel, {:#?}", self),
        };
        unsafe {
            self.afd.cancel(&mut *self.iosb)?;
        }
        self.poll_status = SockPollStatus::Cancelled;
        self.pending_evts = 0;
        Ok(())
    }

    // This is the function called from the overlapped using as Arc<Mutex<SockState>>. Watch out for reference counting.
    fn feed_event(&mut self) -> Option<Event> {
        self.poll_status = SockPollStatus::Idle;
        self.pending_evts = 0;

        let mut afd_events = 0;
        // We use the status info in IO_STATUS_BLOCK to determine the socket poll status. It is unsafe to use a pointer of IO_STATUS_BLOCK.
        unsafe {
            if self.delete_pending {
                return None;
            } else if self.iosb.u.Status == STATUS_CANCELLED {
                /* The poll request was cancelled by CancelIoEx. */
            } else if !NT_SUCCESS(self.iosb.u.Status) {
                /* The overlapped request itself failed in an unexpected way. */
                afd_events = afd::POLL_CONNECT_FAIL;
            } else if self.poll_info.number_of_handles < 1 {
                /* This poll operation succeeded but didn't report any socket events. */
            } else if self.poll_info.handles[0].events & afd::POLL_LOCAL_CLOSE != 0 {
                /* The poll operation reported that the socket was closed. */
                self.mark_delete();
                return None;
            } else {
                afd_events = self.poll_info.handles[0].events;
            }
        }

        afd_events &= self.user_evts;

        if afd_events == 0 {
            return None;
        }

        // In mio, we have to simulate Edge-triggered behavior to match API usage.
        // The strategy here is to intercept all read/write from user that could cause WouldBlock usage,
        // then reregister the socket to reset the interests.

        // Reset readable event
        if (afd_events & interests_to_afd_flags(Interest::READABLE)) != 0 {
            self.user_evts &= !(interests_to_afd_flags(Interest::READABLE));
        }
        // Reset writable event
        if (afd_events & interests_to_afd_flags(Interest::WRITABLE)) != 0 {
            self.user_evts &= !interests_to_afd_flags(Interest::WRITABLE);
        }

        Some(Event {
            data: self.user_data,
            flags: afd_events,
        })
    }

    pub fn is_pending_deletion(&self) -> bool {
        self.delete_pending
    }

    pub fn mark_delete(&mut self) {
        if !self.delete_pending {
            if let SockPollStatus::Pending = self.poll_status {
                drop(self.cancel());
            }

            self.delete_pending = true;
        }
    }

    fn has_error(&self) -> bool {
        self.error.is_some()
    }

    /// True if need to be added on update queue, false otherwise.
    fn set_event(&mut self, ev: Event) -> bool {
        /* afd::POLL_CONNECT_FAIL and afd::POLL_ABORT are always reported, even when not requested by the caller. */
        let events = ev.flags | afd::POLL_CONNECT_FAIL | afd::POLL_ABORT;

        self.user_evts = events;
        self.user_data = ev.data;

        (events & !self.pending_evts) != 0
    }
}

#[derive(Debug)]
struct Win32Event(HANDLE);

unsafe impl Send for Win32Event {}
unsafe impl Sync for Win32Event {}

impl Win32Event {
    fn new() -> io::Result<Win32Event> {
        let event = unsafe {
            CreateEventA(
                ptr::null_mut(), /* no security attributes */
                0,               /* not manual reset */
                0,               /* initially unset */
                ptr::null(),     /* unnamed */
            )
        };
        if event.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Win32Event(event))
        }
    }

    fn set(&self) -> io::Result<()> {
        if unsafe { SetEvent(self.0) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl Drop for Win32Event {
    fn drop(&mut self) {
        // ignore error
        unsafe { CloseHandle(self.0) };
    }
}

#[derive(Debug)]
pub struct FallbackSockState {
    cp: Arc<CompletionPort>,
    raw_socket: RawSocket,
    token: Token,
    interests: u32,
    pending: u32,
    /// Used to notify the thread to update its event flags, or possibly quit
    notify_event: Arc<Win32Event>,
    shutdown: bool,
}

cfg_net! {
    use winapi::um::winsock2::{
        ioctlsocket, WSACreateEvent, WSAEnumNetworkEvents, WSAEventSelect,
        WSAWaitForMultipleEvents, FD_ACCEPT, FD_CLOSE, FD_CLOSE_BIT, FD_CONNECT, FD_CONNECT_BIT,
        FD_READ, FD_WRITE, FIONBIO, SOCKET, WSANETWORKEVENTS, WSA_INFINITE, WSA_INVALID_EVENT,
        WSA_WAIT_FAILED,
    };

    impl FallbackSockState {
        fn new(
            raw_socket: RawSocket,
            token: Token,
            interests: Interest,
            cp: Arc<CompletionPort>,
        ) -> io::Result<FallbackSockState> {
            Ok(FallbackSockState {
                cp,
                raw_socket,
                token,
                interests: interests_to_afd_flags(interests),
                pending: 0,
                notify_event: Arc::new(Win32Event::new()?),
                shutdown: false,
            })
        }

        fn start_poll_thread(&mut self, self_arc: &Pin<Arc<Mutex<SockState>>>) -> io::Result<()> {
            assert!(!self.shutdown);
            let notify_event = self.notify_event.clone();
            let socket_event = unsafe { WSACreateEvent() };
            if socket_event == WSA_INVALID_EVENT {
                return Err(io::Error::last_os_error());
            }
            let socket_event = Win32Event(socket_event);
            let raw_socket = self.raw_socket;
            let self_arc = self_arc.clone();
            thread::spawn(move || {
                let mut guard = self_arc.lock().unwrap();
                if guard.expect_fallback_mut().shutdown {
                    return;
                }

                loop {
                    let interests = guard.expect_fallback_mut().interests;
                    let mut event_flags = 0;
                    if (interests & afd::POLL_SEND) != 0 {
                        event_flags |= FD_WRITE;
                    }
                    if (interests & afd::POLL_RECEIVE) != 0 {
                        event_flags |= FD_READ;
                    }
                    if (interests & afd::POLL_ACCEPT) != 0 {
                        event_flags |= FD_ACCEPT;
                    }
                    if (interests & (afd::POLL_ABORT | afd::POLL_DISCONNECT)) != 0 {
                        event_flags |= FD_CLOSE;
                    }
                    if (interests & (afd::POLL_SEND | afd::POLL_CONNECT_FAIL)) != 0 {
                        event_flags |= FD_CONNECT;
                    }
                    if unsafe { WSAEventSelect(raw_socket as SOCKET, socket_event.0, event_flags) }
                        == SOCKET_ERROR
                    {
                        log::error!("WSAEventSelect failed: {:?}", io::Error::last_os_error());
                        return;
                    }

                    drop(guard);

                    let events = [notify_event.0, socket_event.0];
                    if unsafe {
                        WSAWaitForMultipleEvents(
                            events.len() as u32,
                            &events as *const [_; 2] as *const _,
                            0, /* fWaitAll */
                            WSA_INFINITE,
                            0, /* fAlertable */
                        )
                    } == WSA_WAIT_FAILED
                    {
                        log::error!(
                            "WSAWaitForMultipleEvents failed: {:?}",
                            io::Error::last_os_error()
                        );
                        return;
                    }

                    // Before doing anything else, check if we need to stop.
                    guard = self_arc.lock().unwrap();
                    let this = guard.expect_fallback_mut();
                    if this.shutdown {
                        return;
                    }

                    // Read events.
                    let mut events: WSANETWORKEVENTS = unsafe { std::mem::zeroed() };
                    if unsafe {
                        WSAEnumNetworkEvents(
                            raw_socket as SOCKET,
                            socket_event.0,
                            &mut events as *mut _,
                        )
                    } == SOCKET_ERROR
                    {
                        log::error!(
                            "WSAEnumNetworkEvents failed: {:?}",
                            io::Error::last_os_error()
                        );
                        return;
                    }
                    let mut translated_events = 0;
                    if (events.lNetworkEvents & FD_WRITE) != 0 {
                        translated_events |= afd::POLL_SEND;
                    }
                    if (events.lNetworkEvents & FD_READ) != 0 {
                        translated_events |= afd::POLL_RECEIVE;
                    }
                    if (events.lNetworkEvents & FD_ACCEPT) != 0 {
                        translated_events |= afd::POLL_ACCEPT;
                    }
                    if (events.lNetworkEvents & FD_CLOSE) != 0 {
                        if events.iErrorCode[FD_CLOSE_BIT as usize] != 0 {
                            translated_events |= afd::POLL_ABORT;
                        } else {
                            translated_events |= afd::POLL_DISCONNECT;
                        }
                    }
                    if (events.lNetworkEvents & FD_CONNECT) != 0 {
                        if events.iErrorCode[FD_CONNECT_BIT as usize] != 0 {
                            translated_events |= afd::POLL_CONNECT_FAIL;
                        } else {
                            translated_events |= afd::POLL_SEND;
                        }
                    }

                    // restrict our attention to events that are still requested
                    translated_events &= this.interests;

                    // clear interest for this event
                    this.interests &= !translated_events;
                    this.pending |= translated_events;

                    // signal the main event loop
                    let overlapped = into_overlapped(self_arc.clone()) as *mut _;
                    if let Err(e) = this
                        .cp
                        .post(CompletionStatus::new(0, this.token.0, overlapped))
                    {
                        log::error!("CompletionPort::post error: {:?}", e);
                        break;
                    }
                }
            });
            Ok(())
        }

        fn reregister(&mut self, token: Token, interests: Interest) -> io::Result<()> {
            self.token = token;
            let flags = interests_to_afd_flags(interests);
            let old = self.interests;
            self.interests = flags;
            // If there are queued events that are no longer desired, discard them.
            self.pending &= flags;
            if self.interests != old {
                self.notify_event.set()?;
            }
            Ok(())
        }

        fn feed_event(&mut self) -> Option<Event> {
            if self.pending != 0 && !self.shutdown {
                let flags = self.pending;
                self.pending = 0;
                Some(Event {
                    flags,
                    data: self.token.0 as u64,
                })
            } else {
                None
            }
        }

        fn mark_delete(&mut self) {
            if !self.shutdown {
                self.shutdown = true;
                self.notify_event.set().expect("SetEvent failed");
                // Detach the socket from the socket event.
                if unsafe { WSAEventSelect(self.raw_socket as SOCKET, ptr::null_mut(), 0) }
                    == SOCKET_ERROR
                {
                    log::error!("WSAEventSelect failed: {:?}", io::Error::last_os_error());
                }
                // Attempt to re-mark the socket non-blocking. This resets the
                // cached edge triggers in case the socket is later registered
                // again.
                if let Err(e) = syscall!(
                    ioctlsocket(self.raw_socket as SOCKET, FIONBIO, &mut 1),
                    PartialEq::ne,
                    0
                ) {
                    log::error!("ioctl(FIONBIO) failed: {:?}", e);
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum SockState {
    Afd(AfdSockState),
    Fallback(FallbackSockState),
}

cfg_net! {
    impl SockState {
        fn new(
            raw_socket: RawSocket,
            base_socket: RawSocket,
            afd: Arc<Afd>,
            event: Event,
        ) -> io::Result<SockState> {
            let mut state = AfdSockState {
                iosb: IoStatusBlock::zeroed(),
                poll_info: AfdPollInfo::zeroed(),
                afd,
                raw_socket,
                base_socket,
                user_evts: 0,
                pending_evts: 0,
                user_data: 0,
                poll_status: SockPollStatus::Idle,
                delete_pending: false,
                error: None,
                pinned: PhantomPinned,
            };
            state.set_event(event);
            Ok(SockState::Afd(state))
        }

        fn fallback(
            raw_socket: RawSocket,
            token: Token,
            interests: Interest,
            cp: Arc<CompletionPort>,
        ) -> io::Result<SockState> {
            Ok(SockState::Fallback(FallbackSockState::new(
                raw_socket, token, interests, cp,
            )?))
        }

        fn is_afd(&self) -> bool {
            match self {
                SockState::Afd(_) => true,
                _ => false,
            }
        }

        fn expect_afd_mut(&mut self) -> &mut AfdSockState {
            match self {
                SockState::Afd(afd) => afd,
                SockState::Fallback(_) => panic!("Expected AFD sock state"),
            }
        }

        fn expect_fallback_mut(&mut self) -> &mut FallbackSockState {
            match self {
                SockState::Afd(_) => panic!("Expected fallback sock state"),
                SockState::Fallback(fallback) => fallback,
            }
        }

        fn feed_event(&mut self) -> Option<Event> {
            match self {
                SockState::Afd(afd) => afd.feed_event(),
                SockState::Fallback(fallback) => fallback.feed_event(),
            }
        }

        pub(super) fn mark_delete(&mut self) {
            match self {
                SockState::Afd(afd) => afd.mark_delete(),
                SockState::Fallback(fallback) => fallback.mark_delete(),
            }
        }
    }

    impl Drop for SockState {
        fn drop(&mut self) {
            self.mark_delete();
        }
    }
}

/// Converts the pointer to a `SockState` into a raw pointer.
/// To revert see `from_overlapped`.
fn into_overlapped(sock_state: Pin<Arc<Mutex<SockState>>>) -> PVOID {
    let overlapped_ptr: *const Mutex<SockState> =
        unsafe { Arc::into_raw(Pin::into_inner_unchecked(sock_state)) };
    overlapped_ptr as *mut _
}

/// Convert a raw overlapped pointer into a reference to `SockState`.
/// Reverts `into_overlapped`.
fn from_overlapped(ptr: *mut OVERLAPPED) -> Pin<Arc<Mutex<SockState>>> {
    let sock_ptr: *const Mutex<SockState> = ptr as *const _;
    unsafe { Pin::new_unchecked(Arc::from_raw(sock_ptr)) }
}

/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associated with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

/// Windows implementaion of `sys::Selector`
///
/// Edge-triggered event notification is simulated by resetting internal event flag of each socket state `SockState`
/// and setting all events back by intercepting all requests that could cause `io::ErrorKind::WouldBlock` happening.
///
/// This selector is currently only support socket due to `Afd` driver is winsock2 specific.
#[derive(Debug)]
pub struct Selector {
    #[cfg(debug_assertions)]
    id: usize,

    inner: Arc<SelectorInner>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        SelectorInner::new().map(|inner| {
            #[cfg(debug_assertions)]
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;
            Selector {
                #[cfg(debug_assertions)]
                id,
                inner: Arc::new(inner),
            }
        })
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        Ok(Selector {
            #[cfg(debug_assertions)]
            id: self.id,
            inner: Arc::clone(&self.inner),
        })
    }

    /// # Safety
    ///
    /// This requires a mutable reference to self because only a single thread
    /// can poll IOCP at a time.
    pub fn select(&mut self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.select(events, timeout)
    }

    pub(super) fn clone_port(&self) -> Arc<CompletionPort> {
        self.inner.cp.clone()
    }
}

cfg_net! {
    use super::InternalState;
    use crate::Token;

    impl Selector {
        pub(super) fn register(
            &self,
            socket: RawSocket,
            token: Token,
            interests: Interest,
        ) -> io::Result<InternalState> {
            SelectorInner::register(&self.inner, socket, token, interests)
        }

        pub(super) fn reregister(
            &self,
            state: Pin<Arc<Mutex<SockState>>>,
            token: Token,
            interests: Interest,
        ) -> io::Result<()> {
            self.inner.reregister(state, token, interests)
        }

        #[cfg(debug_assertions)]
        pub fn id(&self) -> usize {
            self.id
        }
    }
}

#[derive(Debug)]
pub struct SelectorInner {
    cp: Arc<CompletionPort>,
    update_queue: Mutex<VecDeque<Pin<Arc<Mutex<SockState>>>>>,
    afd_group: AfdGroup,
    is_polling: AtomicBool,
}

// We have ensured thread safety by introducing lock manually.
unsafe impl Sync for SelectorInner {}

impl SelectorInner {
    pub fn new() -> io::Result<SelectorInner> {
        CompletionPort::new(0).map(|cp| {
            let cp = Arc::new(cp);
            let cp_afd = Arc::clone(&cp);

            SelectorInner {
                cp,
                update_queue: Mutex::new(VecDeque::new()),
                afd_group: AfdGroup::new(cp_afd),
                is_polling: AtomicBool::new(false),
            }
        })
    }

    /// # Safety
    ///
    /// May only be calling via `Selector::select`.
    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();

        if timeout.is_none() {
            loop {
                let len = self.select2(&mut events.statuses, &mut events.events, None)?;
                if len == 0 {
                    continue;
                }
                return Ok(());
            }
        } else {
            self.select2(&mut events.statuses, &mut events.events, timeout)?;
            return Ok(());
        }
    }

    pub fn select2(
        &self,
        statuses: &mut [CompletionStatus],
        events: &mut Vec<Event>,
        timeout: Option<Duration>,
    ) -> io::Result<usize> {
        assert_eq!(self.is_polling.swap(true, Ordering::AcqRel), false);

        unsafe { self.update_sockets_events() }?;

        let result = self.cp.get_many(statuses, timeout);

        self.is_polling.store(false, Ordering::Relaxed);

        match result {
            Ok(iocp_events) => Ok(unsafe { self.feed_events(events, iocp_events) }),
            Err(ref e) if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) => Ok(0),
            Err(e) => Err(e),
        }
    }

    unsafe fn update_sockets_events(&self) -> io::Result<()> {
        let mut update_queue = self.update_queue.lock().unwrap();
        for sock in update_queue.iter_mut() {
            let mut sock_internal = sock.lock().unwrap();
            let sock_internal = sock_internal.expect_afd_mut();
            if !sock_internal.is_pending_deletion() {
                sock_internal.update(&sock)?;
            }
        }

        // remove all sock which do not have error, they have afd op pending
        update_queue.retain(|sock| sock.lock().unwrap().expect_afd_mut().has_error());

        self.afd_group.release_unused_afd();
        Ok(())
    }

    // It returns processed count of iocp_events rather than the events itself.
    unsafe fn feed_events(
        &self,
        events: &mut Vec<Event>,
        iocp_events: &[CompletionStatus],
    ) -> usize {
        let mut n = 0;
        let mut update_queue = self.update_queue.lock().unwrap();
        for iocp_event in iocp_events.iter() {
            if iocp_event.overlapped().is_null() {
                // `Waker` event, we'll add a readable event to match the other platforms.
                events.push(Event {
                    flags: afd::POLL_RECEIVE,
                    data: iocp_event.token() as u64,
                });
                n += 1;
                continue;
            }

            let sock_state = from_overlapped(iocp_event.overlapped());
            let mut sock_guard = sock_state.lock().unwrap();
            match sock_guard.feed_event() {
                Some(e) => {
                    events.push(e);
                    n += 1;
                }
                None => {}
            }

            if let SockState::Afd(ref afd_sock) = *sock_guard {
                if !afd_sock.is_pending_deletion() {
                    update_queue.push_back(sock_state.clone());
                }
            }
        }
        self.afd_group.release_unused_afd();
        n
    }
}

cfg_net! {
    use std::mem::size_of;
    use std::ptr::null_mut;
    use winapi::um::mswsock::SIO_BASE_HANDLE;
    use winapi::um::winsock2::{WSAIoctl, SOCKET_ERROR};

    impl SelectorInner {
        fn register(
            this: &Arc<Self>,
            socket: RawSocket,
            token: Token,
            interests: Interest,
        ) -> io::Result<InternalState> {
            let sock_state = match get_base_socket(socket) {
                Ok(base_socket) => {
                    let afd = this.afd_group.acquire()?;
                    let event = Event {
                        flags: interests_to_afd_flags(interests),
                        data: token.0 as u64,
                    };
                    let sock =
                        Arc::pin(Mutex::new(SockState::new(socket, base_socket, afd, event)?));
                    this.queue_state(sock.clone());
                    unsafe {
                        this.update_sockets_events_if_polling()?;
                    }
                    sock
                }
                Err(e) => {
                    // This can happen when the socket is not a MSAFD socket,
                    // often because there is some other stuff on the system
                    // intercepting TCP connections. Fall back to using an
                    // inefficient emulation.
                    log::warn!("get_base_socket failed: {:?}", e);
                    let sock = Arc::pin(Mutex::new(SockState::fallback(
                        socket,
                        token,
                        interests,
                        this.cp.clone(),
                    )?));
                    sock.lock()
                        .unwrap()
                        .expect_fallback_mut()
                        .start_poll_thread(&sock)?;
                    sock
                }
            };

            let state = InternalState {
                selector: this.clone(),
                token,
                interests,
                sock_state,
            };

            Ok(state)
        }

        // Directly accessed in `IoSourceState::do_io`.
        pub(super) fn reregister(
            &self,
            state: Pin<Arc<Mutex<SockState>>>,
            token: Token,
            interests: Interest,
        ) -> io::Result<()> {
            let mut state_guard = state.lock().unwrap();
            match *state_guard {
                SockState::Afd(ref mut afd) => {
                    afd.set_event(Event {
                        flags: interests_to_afd_flags(interests),
                        data: token.0 as u64,
                    });

                    drop(state_guard);

                    // FIXME: a sock which has_error true should not be re-added to
                    // the update queue because it's already there.
                    self.queue_state(state);
                    unsafe { self.update_sockets_events_if_polling() }
                }
                SockState::Fallback(ref mut fallback) => fallback.reregister(token, interests),
            }
        }

        /// This function is called by register() and reregister() to start an
        /// IOCTL_AFD_POLL operation corresponding to the registered events, but
        /// only if necessary.
        ///
        /// Since it is not possible to modify or synchronously cancel an AFD_POLL
        /// operation, and there can be only one active AFD_POLL operation per
        /// (socket, completion port) pair at any time, it is expensive to change
        /// a socket's event registration after it has been submitted to the kernel.
        ///
        /// Therefore, if no other threads are polling when interest in a socket
        /// event is (re)registered, the socket is added to the 'update queue', but
        /// the actual syscall to start the IOCTL_AFD_POLL operation is deferred
        /// until just before the GetQueuedCompletionStatusEx() syscall is made.
        ///
        /// However, when another thread is already blocked on
        /// GetQueuedCompletionStatusEx() we tell the kernel about the registered
        /// socket event(s) immediately.
        unsafe fn update_sockets_events_if_polling(&self) -> io::Result<()> {
            if self.is_polling.load(Ordering::Acquire) {
                self.update_sockets_events()
            } else {
                Ok(())
            }
        }

        fn queue_state(&self, sock_state: Pin<Arc<Mutex<SockState>>>) {
            debug_assert!(sock_state.lock().unwrap().is_afd());
            let mut update_queue = self.update_queue.lock().unwrap();
            update_queue.push_back(sock_state);
        }
    }

    fn get_base_socket(raw_socket: RawSocket) -> io::Result<RawSocket> {
        let mut base_socket: RawSocket = 0;
        let mut bytes: u32 = 0;

        unsafe {
            if WSAIoctl(
                raw_socket as usize,
                SIO_BASE_HANDLE,
                null_mut(),
                0,
                &mut base_socket as *mut _ as PVOID,
                size_of::<RawSocket>() as u32,
                &mut bytes,
                null_mut(),
                None,
            ) == SOCKET_ERROR
            {
                Err(io::Error::last_os_error())
            } else {
                Ok(base_socket)
            }
        }
    }
}

impl Drop for SelectorInner {
    fn drop(&mut self) {
        loop {
            let events_num: usize;
            let mut statuses: [CompletionStatus; 1024] = [CompletionStatus::zero(); 1024];

            let result = self
                .cp
                .get_many(&mut statuses, Some(std::time::Duration::from_millis(0)));
            match result {
                Ok(iocp_events) => {
                    events_num = iocp_events.iter().len();
                    for iocp_event in iocp_events.iter() {
                        if !iocp_event.overlapped().is_null() {
                            // drain sock state to release memory of Arc reference
                            let _sock_state = from_overlapped(iocp_event.overlapped());
                        }
                    }
                }

                Err(_) => {
                    break;
                }
            }

            if events_num == 0 {
                // continue looping until all completion statuses have been drained
                break;
            }
        }

        self.afd_group.release_unused_afd();
    }
}

fn interests_to_afd_flags(interests: Interest) -> u32 {
    let mut flags = 0;

    if interests.is_readable() {
        // afd::POLL_DISCONNECT for is_read_hup()
        flags |= afd::POLL_RECEIVE | afd::POLL_ACCEPT | afd::POLL_DISCONNECT;
    }

    if interests.is_writable() {
        flags |= afd::POLL_SEND;
    }

    flags
}
