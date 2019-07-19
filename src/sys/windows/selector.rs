use std::collections::VecDeque;
use std::io;
use std::mem::size_of;
use std::pin::Pin;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::os::windows::io::{AsRawSocket, RawSocket};

use winapi::shared::ntdef::NT_SUCCESS;
use winapi::shared::ntdef::{HANDLE, PVOID};
use winapi::shared::ntstatus::STATUS_CANCELLED;
use winapi::shared::winerror::{ERROR_INVALID_HANDLE, ERROR_IO_PENDING};
use winapi::um::mswsock::SIO_BASE_HANDLE;
use winapi::um::winsock2::{WSAIoctl, INVALID_SOCKET, SOCKET_ERROR};

use miow::iocp::{CompletionPort, CompletionStatus};

use crate::sys::Events;
use crate::{Interests, Token};

use super::afd::{Afd, AfdPollInfo};
use super::afd::{
    AFD_POLL_ABORT, AFD_POLL_ACCEPT, AFD_POLL_CONNECT_FAIL, AFD_POLL_DISCONNECT,
    AFD_POLL_LOCAL_CLOSE, AFD_POLL_RECEIVE, AFD_POLL_RECEIVE_EXPEDITED, AFD_POLL_SEND,
    KNOWN_AFD_EVENTS,
};
use super::io_status_block::IoStatusBlock;
use super::Event;
use super::SocketState;

const POLL_GROUP__MAX_GROUP_SIZE: usize = 32;

#[derive(PartialEq, Debug, Clone, Copy)]
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

        if self.poll_status == SockPollStatus::Pending
            && (self.user_evts & KNOWN_AFD_EVENTS & !self.pending_evts) == 0
        {
            /* All the events the user is interested in are already being monitored by
             * the pending poll operation. It might spuriously complete because of an
             * event that we're no longer interested in; when that happens we'll submit
             * a new poll operation with the updated event mask. */
        } else if self.poll_status == SockPollStatus::Pending {
            /* A poll operation is already pending, but it's not monitoring for all the
             * events that the user is interested in. Therefore, cancel the pending
             * poll operation; when we receive it's completion package, a new poll
             * operation will be submitted with the correct event mask. */
            self.cancel()?;
            return Ok(());
        } else if self.poll_status == SockPollStatus::Cancelled {
            /* The poll operation has already been cancelled, we're still waiting for
             * it to return. For now, there's nothing that needs to be done. */
            return Ok(());
        } else if self.poll_status == SockPollStatus::Idle {
            /* No poll operation is pending; start one. */
            self.poll_info.exclusive = 0;
            self.poll_info.number_of_handles = 1;
            unsafe {
                *self.poll_info.timeout.QuadPart_mut() = std::i64::MAX;
            }
            self.poll_info.handles[0].handle = self.base_socket as HANDLE;
            self.poll_info.handles[0].status = 0;
            self.poll_info.handles[0].events = self.user_evts;

            let overlapped = Arc::into_raw(self_arc.clone()) as *const _ as PVOID;
            let result = unsafe {
                self.afd
                    .poll(&mut self.poll_info, (*self.iosb).as_mut_ptr(), overlapped)
            };
            if let Err(e) = result {
                if let Some(code) = e.raw_os_error() {
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
            }

            self.poll_status = SockPollStatus::Pending;
            self.pending_evts = self.user_evts;
        } else {
            unreachable!();
        }
        Ok(())
    }

    fn cancel(&mut self) -> io::Result<()> {
        assert!(self.poll_status == SockPollStatus::Pending);
        unsafe {
            self.afd.cancel((*self.iosb).as_mut_ptr())?;
        }
        self.poll_status = SockPollStatus::Cancelled;
        self.pending_evts = 0;
        Ok(())
    }

    fn mark_delete(&mut self) {
        if !self.delete_pending {
            if self.poll_status == SockPollStatus::Pending {
                drop(self.cancel());
            }

            self.delete_pending = true;
        }
    }

    fn feed_event(&mut self) -> Option<Event> {
        let mut afd_events = 0;
        self.poll_status = SockPollStatus::Idle;
        self.pending_evts = 0;

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

        // Reset readable event
        if (afd_events & (KNOWN_AFD_EVENTS & !AFD_POLL_SEND)) != 0 {
            self.user_evts &= !(afd_events & (KNOWN_AFD_EVENTS & !AFD_POLL_SEND));
        }
        // Reset writable event
        if (afd_events & AFD_POLL_SEND) != 0 {
            self.user_evts &= !AFD_POLL_SEND;
        }

        Some(Event {
            data: self.user_data,
            flags: afd_events,
        })
    }

    fn is_pending_deletion(&self) -> bool {
        self.delete_pending
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

    pub(super) fn inner(&self) -> &SelectorInner {
        &self.inner
    }

    pub(super) fn clone_inner(&self) -> Arc<SelectorInner> {
        self.inner.clone()
    }
}

#[derive(Debug)]
pub struct SelectorInner {
    cp: CompletionPort,
    active_poll_count: AtomicUsize,
    update_queue: Mutex<VecDeque<Arc<Mutex<SockState>>>>,
    afd_group: Mutex<Vec<Arc<Afd>>>,
}

impl SelectorInner {
    pub fn new() -> io::Result<SelectorInner> {
        CompletionPort::new(0).map(|cp| SelectorInner {
            cp: cp,
            active_poll_count: AtomicUsize::new(0),
            update_queue: Mutex::new(VecDeque::new()),
            afd_group: Mutex::new(Vec::new()),
        })
    }

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();

        self.update_sockets_events()?;

        self.active_poll_count.fetch_add(1, Ordering::SeqCst);

        let result = self.cp.get_many(&mut events.statuses, timeout);

        self.active_poll_count.fetch_sub(1, Ordering::SeqCst);

        if let Err(e) = result {
            use winapi::shared::winerror::WAIT_TIMEOUT;
            if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) {
                return Ok(());
            }
            return Err(e);
        }

        self.feed_events(&mut events.events, result.unwrap());
        Ok(())
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
        self.add_socket_to_update_queue(socket);
        self.update_sockets_events_if_polling()?;

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
        self.add_socket_to_update_queue(socket);
        self.update_sockets_events_if_polling()?;

        Ok(())
    }

    pub fn deregister<S: SocketState>(&self, socket: &S) -> io::Result<()> {
        if socket.get_sock_state().is_none() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        socket.set_sock_state(None);
        self._release_unused_afd();
        Ok(())
    }

    pub fn port(&self) -> &CompletionPort {
        &self.cp
    }

    pub fn mark_delete_socket(&self, sock_state: &mut SockState) {
        sock_state.mark_delete();
    }

    fn update_sockets_events(&self) -> io::Result<()> {
        {
            let mut update_queue = self.update_queue.lock().unwrap();
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
        }
        self._release_unused_afd();
        Ok(())
    }

    fn update_sockets_events_if_polling(&self) -> io::Result<()> {
        if self.active_poll_count.load(Ordering::SeqCst) > 0 {
            return self.update_sockets_events();
        }
        Ok(())
    }

    fn feed_events(&self, events: &mut Vec<Event>, iocp_events: &[CompletionStatus]) {
        {
            let mut update_queue = self.update_queue.lock().unwrap();
            for iocp_event in iocp_events.iter() {
                if iocp_event.overlapped() as usize == 0 {
                    events.push(Event {
                        flags: AFD_POLL_RECEIVE,
                        data: iocp_event.token() as u64,
                    });
                    continue;
                }
                let sock =
                    unsafe { Arc::from_raw(iocp_event.overlapped() as *mut Mutex<SockState>) };
                let mut sock_guard = sock.lock().unwrap();
                match sock_guard.feed_event() {
                    Some(e) => {
                        events.push(e);
                    }
                    None => {}
                }
                if !sock_guard.is_pending_deletion() {
                    update_queue.push_back(sock.clone());
                }
            }
        }
        self._release_unused_afd();
    }

    fn _acquire_afd(&self) -> io::Result<Arc<Afd>> {
        let mut need_alloc = false;
        {
            let afd_group = self.afd_group.lock().unwrap();
            if afd_group.len() == 0 {
                need_alloc = true;
            } else {
                // + 1 reference in Vec
                if Arc::strong_count(afd_group.last().unwrap()) >= POLL_GROUP__MAX_GROUP_SIZE + 1 {
                    need_alloc = true;
                }
            }
        }
        if need_alloc {
            self._alloc_afd_group()?;
        }
        match self.afd_group.lock().unwrap().last() {
            Some(rc) => Ok(rc.clone()),
            None => unreachable!(),
        }
    }

    fn _release_unused_afd(&self) {
        let mut afd_group = self.afd_group.lock().unwrap();
        afd_group.retain(|g| Arc::strong_count(&g) > 1);
    }

    fn _alloc_afd_group(&self) -> io::Result<()> {
        let mut afd_group = self.afd_group.lock().unwrap();
        let afd = Afd::new(&self.cp)?;
        let rc = Arc::new(afd);
        afd_group.push(rc);
        Ok(())
    }

    fn _alloc_sock_for_rawsocket(
        &self,
        raw_socket: RawSocket,
    ) -> io::Result<Arc<Mutex<SockState>>> {
        Ok(Arc::new(Mutex::new(SockState::new(
            raw_socket,
            self._acquire_afd()?,
        )?)))
    }

    fn add_socket_to_update_queue<S: SocketState>(&self, socket: &S) {
        let sock_state = socket.get_sock_state().unwrap();
        let mut update_queue = self.update_queue.lock().unwrap();
        update_queue.push_back(sock_state);
    }
}

fn interests_to_afd_flags(interests: Interests) -> u32 {
    let mut flags = 0;

    if interests.is_readable() {
        flags |=
            AFD_POLL_RECEIVE | AFD_POLL_RECEIVE_EXPEDITED | AFD_POLL_ACCEPT | AFD_POLL_DISCONNECT;
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
