use super::afd::{Afd, AfdPollInfo};
use super::afd::{
    AFD_POLL_ABORT, AFD_POLL_ACCEPT, AFD_POLL_CONNECT_FAIL, AFD_POLL_DISCONNECT,
    AFD_POLL_LOCAL_CLOSE, AFD_POLL_RECEIVE, AFD_POLL_SEND, KNOWN_AFD_EVENTS,
};
use super::io_status_block::IoStatusBlock;
use super::Event;
use super::SocketState;
use crate::sys::Events;
use crate::{Interests, Token};

use miow::iocp::{CompletionPort, CompletionStatus};
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawSocket, RawSocket};
use std::pin::Pin;
use std::ptr::null_mut;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winapi::shared::ntdef::NT_SUCCESS;
use winapi::shared::ntdef::{HANDLE, PVOID};
use winapi::shared::ntstatus::STATUS_CANCELLED;
use winapi::shared::winerror::{ERROR_INVALID_HANDLE, ERROR_IO_PENDING, WAIT_TIMEOUT};
use winapi::um::mswsock::SIO_BASE_HANDLE;
use winapi::um::winsock2::{WSAIoctl, INVALID_SOCKET, SOCKET_ERROR};

const POLL_GROUP__MAX_GROUP_SIZE: usize = 32;

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
            None => unreachable!(),
        }
    }

    pub fn release_unused_afd(&self) {
        let mut afd_group = self.afd_group.lock().unwrap();
        afd_group.retain(|g| Arc::strong_count(&g) > 1);
    }

    fn _alloc_afd_group(&self, afd_group: &mut Vec<Arc<Afd>>) -> io::Result<()> {
        let afd = Afd::new(&self.cp)?;
        let arc = Arc::new(afd);
        afd_group.push(arc);
        Ok(())
    }
}

/// This is the deallocation wrapper for overlapped pointer.
/// In case of error or status changing before the overlapped pointer is actually used(or not even being used),
/// this wrapper will decrease the reference count of Arc if being dropped.
/// Remember call `forget` if you have used the Arc, or you could decrease the reference count by two causing double free.
#[derive(Debug)]
struct OverlappedArcWrapper<T>(*const T);

unsafe impl<T> Send for OverlappedArcWrapper<T> {}

impl<T> OverlappedArcWrapper<T> {
    fn new(arc: &Arc<T>) -> OverlappedArcWrapper<T> {
        OverlappedArcWrapper(Arc::into_raw(arc.clone()))
    }

    fn forget(&mut self) {
        self.0 = 0 as *const T;
    }

    fn get_ptr(&self) -> *const T {
        self.0
    }
}

impl<T> Drop for OverlappedArcWrapper<T> {
    fn drop(&mut self) {
        if self.0 as usize == 0 {
            return;
        }
        drop(unsafe { Arc::from_raw(self.0) });
    }
}

#[derive(Debug)]
enum SockPollStatus {
    Idle,
    Pending,
    Cancelled,
}

#[derive(Debug)]
pub struct SockState {
    iosb: Pin<Box<IoStatusBlock>>,
    poll_info: AfdPollInfo,
    afd: Arc<Afd>,

    raw_socket: RawSocket,
    base_socket: RawSocket,

    user_evts: u32,
    pending_evts: u32,

    user_data: u64,

    poll_status: SockPollStatus,
    self_wrapped: Option<OverlappedArcWrapper<Mutex<SockState>>>,

    delete_pending: bool,
}

impl SockState {
    fn new(raw_socket: RawSocket, afd: Arc<Afd>) -> io::Result<SockState> {
        Ok(SockState {
            iosb: Pin::new(Box::new(IoStatusBlock::zeroed())),
            poll_info: AfdPollInfo::zeroed(),
            afd,
            raw_socket,
            base_socket: get_base_socket(raw_socket)?,
            user_evts: 0,
            pending_evts: 0,
            user_data: 0,
            poll_status: SockPollStatus::Idle,
            self_wrapped: None,
            delete_pending: false,
        })
    }

    /// True if need to be added on update queue, false otherwise.
    fn set_event(&mut self, ev: Event) -> bool {
        /* AFD_POLL_CONNECT_FAIL and AFD_POLL_ABORT are always reported, even when not requested by the caller. */
        let events = ev.flags | AFD_POLL_CONNECT_FAIL | AFD_POLL_ABORT;

        self.user_evts = events;
        self.user_data = ev.data;

        (events & !self.pending_evts) != 0
    }

    fn update(&mut self, self_arc: &Arc<Mutex<SockState>>) -> io::Result<()> {
        assert!(!self.delete_pending);

        if let SockPollStatus::Pending = self.poll_status {
            if (self.user_evts & KNOWN_AFD_EVENTS & !self.pending_evts) == 0 {
                /* All the events the user is interested in are already being monitored by
                 * the pending poll operation. It might spuriously complete because of an
                 * event that we're no longer interested in; when that happens we'll submit
                 * a new poll operation with the updated event mask. */
            } else {
                /* A poll operation is already pending, but it's not monitoring for all the
                 * events that the user is interested in. Therefore, cancel the pending
                 * poll operation; when we receive it's completion package, a new poll
                 * operation will be submitted with the correct event mask. */
                self.cancel()?;
            }
        } else if let SockPollStatus::Cancelled = self.poll_status {
            /* The poll operation has already been cancelled, we're still waiting for
             * it to return. For now, there's nothing that needs to be done. */
        } else if let SockPollStatus::Idle = self.poll_status {
            /* No poll operation is pending; start one. */
            self.poll_info.exclusive = 0;
            self.poll_info.number_of_handles = 1;
            unsafe {
                *self.poll_info.timeout.QuadPart_mut() = std::i64::MAX;
            }
            self.poll_info.handles[0].handle = self.base_socket as HANDLE;
            self.poll_info.handles[0].status = 0;
            self.poll_info.handles[0].events = self.user_evts | AFD_POLL_LOCAL_CLOSE;

            let wrapped_overlapped = OverlappedArcWrapper::new(self_arc);
            let overlapped = wrapped_overlapped.get_ptr() as *const _ as PVOID;
            let result = unsafe {
                self.afd
                    .poll(&mut self.poll_info, (*self.iosb).as_mut_ptr(), overlapped)
            };
            if let Err(e) = result {
                let code = e.raw_os_error().unwrap();
                if code == ERROR_IO_PENDING as i32 {
                    /* Overlapped poll operation in progress; this is expected. */
                } else if code == ERROR_INVALID_HANDLE as i32 {
                    /* Socket closed; it'll be dropped. */
                    self.mark_delete();
                    return Ok(());
                } else {
                    return Err(e);
                }
            }

            if self.self_wrapped.is_some() {
                // This shouldn't be happening. We cannot deallocate already pending overlapped before feed_event so we need to stand out here to declare unreachable.
                unreachable!();
            }
            self.poll_status = SockPollStatus::Pending;
            self.self_wrapped = Some(wrapped_overlapped);
            self.pending_evts = self.user_evts;
        } else {
            unreachable!();
        }
        Ok(())
    }

    fn cancel(&mut self) -> io::Result<()> {
        match self.poll_status {
            SockPollStatus::Pending => {}
            _ => unreachable!(),
        };
        unsafe {
            self.afd.cancel((*self.iosb).as_mut_ptr())?;
        }
        self.poll_status = SockPollStatus::Cancelled;
        self.pending_evts = 0;
        Ok(())
    }

    // This is the function called from the overlapped using as Arc<Mutex<SockState>>. Watch out for reference counting.
    fn feed_event(&mut self) -> Option<Event> {
        if self.self_wrapped.is_some() {
            // Forget our arced-self first. We will decrease the reference count by two if we don't do this on overlapped.
            self.self_wrapped.as_mut().unwrap().forget();
            self.self_wrapped = None;
        }

        self.poll_status = SockPollStatus::Idle;
        self.pending_evts = 0;

        let mut afd_events = 0;
        // We use the status info in IO_STATUS_BLOCK to determine the socket poll status. It is unsafe to use a pointer of IO_STATUS_BLOCK.
        unsafe {
            let iosb = &*(*self.iosb).as_ptr();
            if self.delete_pending {
                return None;
            } else if iosb.u.Status == STATUS_CANCELLED {
                /* The poll request was cancelled by CancelIoEx. */
            } else if !NT_SUCCESS(iosb.u.Status) {
                /* The overlapped request itself failed in an unexpected way. */
                afd_events = AFD_POLL_CONNECT_FAIL;
            } else if self.poll_info.number_of_handles < 1 {
                /* This poll operation succeeded but didn't report any socket events. */
            } else if self.poll_info.handles[0].events & AFD_POLL_LOCAL_CLOSE != 0 {
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
        if (afd_events & interests_to_afd_flags(Interests::READABLE)) != 0 {
            self.user_evts &= !(interests_to_afd_flags(Interests::READABLE));
        }
        // Reset writable event
        if (afd_events & interests_to_afd_flags(Interests::WRITABLE)) != 0 {
            self.user_evts &= !interests_to_afd_flags(Interests::WRITABLE);
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
}

impl Drop for SockState {
    fn drop(&mut self) {
        self.mark_delete();
    }
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

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.select(events, timeout)
    }

    pub fn register<S: SocketState + AsRawSocket>(
        &self,
        socket: &S,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.inner.register(socket, token, interests)
    }

    pub fn reregister<S: SocketState>(
        &self,
        socket: &S,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.inner.reregister(socket, token, interests)
    }

    pub fn deregister<S: SocketState>(&self, socket: &S) -> io::Result<()> {
        self.inner.deregister(socket)
    }

    #[cfg(debug_assertions)]
    pub fn id(&self) -> usize {
        self.id
    }

    pub(super) fn clone_inner(&self) -> Arc<SelectorInner> {
        self.inner.clone()
    }
}

#[derive(Debug)]
pub struct SelectorInner {
    lock: Mutex<()>,
    cp: Arc<CompletionPort>,
    active_poll_count: UnsafeCell<usize>,
    update_queue: UnsafeCell<VecDeque<Arc<Mutex<SockState>>>>,
    afd_group: AfdGroup,
}

// We have ensured thread safety by introducing lock manually.
unsafe impl Sync for SelectorInner {}

impl SelectorInner {
    pub fn new() -> io::Result<SelectorInner> {
        CompletionPort::new(0).map(|cp| {
            let cp = Arc::new(cp);
            let cp_afd = Arc::clone(&cp);

            SelectorInner {
                lock: Mutex::new(()),
                cp: cp,
                active_poll_count: UnsafeCell::new(0),
                update_queue: UnsafeCell::new(VecDeque::new()),
                afd_group: AfdGroup::new(cp_afd),
            }
        })
    }

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();

        let mut n = 0;
        let start = Instant::now();

        loop {
            if timeout.is_none() {
                let len = self.select2(&mut events.statuses, &mut events.events, None)?;
                if len == 0 {
                    continue;
                }
                return Ok(());
            } else {
                if n >= events.statuses.len() {
                    return Ok(());
                }
                let timeout = timeout.unwrap();
                let deadline = start + timeout;
                let now = Instant::now();
                if timeout.as_nanos() != 0 {
                    if now >= deadline {
                        return Ok(());
                    }
                    let len = self.select2(
                        &mut events.statuses[n..],
                        &mut events.events,
                        Some(deadline - now),
                    )?;
                    if len == 0 {
                        return Ok(());
                    }
                    n += len;
                } else {
                    self.select2(&mut events.statuses[n..], &mut events.events, Some(timeout))?;
                    return Ok(());
                }
            }
        }
    }

    pub fn select2(
        &self,
        statuses: &mut [CompletionStatus],
        events: &mut Vec<Event>,
        timeout: Option<Duration>,
    ) -> io::Result<usize> {
        {
            let _guard = self.lock.lock().unwrap();

            unsafe {
                self.update_sockets_events()?;

                *self.active_poll_count.get() += 1;
            }
        }

        let result = self.cp.get_many(statuses, timeout);

        {
            let _guard = self.lock.lock().unwrap();

            unsafe {
                *self.active_poll_count.get() -= 1;
            }

            if let Err(e) = result {
                if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) {
                    return Ok(0);
                }
                return Err(e);
            }
            unsafe { Ok(self.feed_events(events, result.unwrap())) }
        }
    }

    pub fn register<S: SocketState + AsRawSocket>(
        &self,
        socket: &S,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        if socket.get_sock_state().is_some() {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }

        let flags = interests_to_afd_flags(interests);

        let sock = self._alloc_sock_for_rawsocket(socket.as_raw_socket())?;
        let event = Event {
            flags,
            data: token.0 as u64,
        };

        {
            sock.lock().unwrap().set_event(event);
        }
        socket.set_sock_state(Some(sock));
        {
            let _guard = self.lock.lock().unwrap();
            unsafe {
                self.add_socket_to_update_queue(socket);
                self.update_sockets_events_if_polling()?;
            }
        }

        Ok(())
    }

    pub fn reregister<S: SocketState>(
        &self,
        socket: &S,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        let flags = interests_to_afd_flags(interests);

        let sock = match socket.get_sock_state() {
            Some(sock) => sock,
            None => return Err(io::Error::from(io::ErrorKind::NotFound)),
        };
        let event = Event {
            flags,
            data: token.0 as u64,
        };

        {
            sock.lock().unwrap().set_event(event);
        }
        {
            let _guard = self.lock.lock().unwrap();
            unsafe {
                self.add_socket_to_update_queue(socket);
                self.update_sockets_events_if_polling()?;
            }
        }

        Ok(())
    }

    pub fn deregister<S: SocketState>(&self, socket: &S) -> io::Result<()> {
        if socket.get_sock_state().is_none() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        socket.set_sock_state(None);
        self.afd_group.release_unused_afd();
        Ok(())
    }

    pub fn port(&self) -> &CompletionPort {
        &self.cp
    }

    unsafe fn update_sockets_events(&self) -> io::Result<()> {
        let update_queue = &mut *self.update_queue.get();
        loop {
            let sock = match update_queue.pop_front() {
                Some(sock) => sock,
                None => break,
            };
            let mut sock_internal = sock.lock().unwrap();
            if !sock_internal.is_pending_deletion() {
                sock_internal.update(&sock).unwrap();
            }
        }
        self.afd_group.release_unused_afd();
        Ok(())
    }

    unsafe fn update_sockets_events_if_polling(&self) -> io::Result<()> {
        let active_poll_count = *self.active_poll_count.get();
        if active_poll_count > 0 {
            return self.update_sockets_events();
        }
        Ok(())
    }

    unsafe fn add_socket_to_update_queue<S: SocketState>(&self, socket: &S) {
        let sock_state = socket.get_sock_state().unwrap();
        let update_queue = &mut *self.update_queue.get();
        update_queue.push_back(sock_state);
    }

    // It returns processed count of iocp_events rather than the events itself.
    unsafe fn feed_events(
        &self,
        events: &mut Vec<Event>,
        iocp_events: &[CompletionStatus],
    ) -> usize {
        let mut n = 0;
        let update_queue = &mut *self.update_queue.get();
        for iocp_event in iocp_events.iter() {
            if iocp_event.overlapped() as usize == 0 {
                events.push(Event {
                    flags: AFD_POLL_RECEIVE,
                    data: iocp_event.token() as u64,
                });
                n += 1;
                continue;
            }
            let sock_arc = Arc::from_raw(iocp_event.overlapped() as *const Mutex<SockState>);
            let mut sock_guard = sock_arc.lock().unwrap();
            match sock_guard.feed_event() {
                Some(e) => {
                    events.push(e);
                }
                None => {}
            }
            n += 1;
            if !sock_guard.is_pending_deletion() {
                update_queue.push_back(sock_arc.clone());
            }
        }
        self.afd_group.release_unused_afd();
        n
    }

    fn _alloc_sock_for_rawsocket(
        &self,
        raw_socket: RawSocket,
    ) -> io::Result<Arc<Mutex<SockState>>> {
        let afd = self.afd_group.acquire()?;
        Ok(Arc::new(Mutex::new(SockState::new(raw_socket, afd)?)))
    }
}

fn interests_to_afd_flags(interests: Interests) -> u32 {
    let mut flags = 0;

    if interests.is_readable() {
        // AFD_POLL_DISCONNECT for is_read_hup()
        flags |= AFD_POLL_RECEIVE | AFD_POLL_ACCEPT | AFD_POLL_DISCONNECT;
    }

    if interests.is_writable() {
        flags |= AFD_POLL_SEND;
    }

    flags
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
            return Err(io::Error::from_raw_os_error(INVALID_SOCKET as i32));
        }
    }
    Ok(base_socket)
}
