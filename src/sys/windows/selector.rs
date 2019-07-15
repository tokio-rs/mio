use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::os::windows::io::{AsRawSocket, RawSocket};

use ntapi::ntioapi::IO_STATUS_BLOCK;
use winapi::shared::ntdef::HANDLE;
use winapi::shared::ntdef::NT_SUCCESS;
use winapi::shared::ntstatus::STATUS_CANCELLED;
use winapi::shared::winerror::{ERROR_INVALID_HANDLE, ERROR_IO_PENDING};
use winapi::um::winsock2::INVALID_SOCKET;

use miow::iocp::{CompletionPort, CompletionStatus};

use crate::sys::Events;
use crate::{Interests, Token};

use super::afd::{eventflags_to_afd_events, Afd, AfdPollInfo};
use super::Event;

const POLL_GROUP__MAX_GROUP_SIZE: usize = 32;

#[derive(PartialEq, Debug, Clone, Copy)]
enum SockPollStatus {
    Idle,
    Pending,
    Cancelled,
}

struct IoStatusBlock(pub IO_STATUS_BLOCK);

impl IoStatusBlock {
    fn zeroed() -> IoStatusBlock {
        IoStatusBlock(unsafe { mem::zeroed::<IO_STATUS_BLOCK>() })
    }
}

impl fmt::Debug for IoStatusBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IO_STATUS_BLOCK")
    }
}

unsafe impl Send for IoStatusBlock {}
unsafe impl Sync for IoStatusBlock {}

#[derive(Debug)]
struct Sock {
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
}

impl Sock {
    fn new(raw_socket: RawSocket, afd: Arc<Afd>) -> io::Result<Sock> {
        unsafe {
            Ok(Sock {
                iosb: IoStatusBlock::zeroed(),
                poll_info: mem::zeroed::<AfdPollInfo>(),
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
    }

    /// True if need to be added on update queue, false otherwise.
    fn set_event(&mut self, ev: Event) -> bool {
        /* EPOLLERR and EPOLLHUP are always reported, even when not requested by the
         * caller. However they are disabled after a event has been reported for a
         * socket for which the EPOLLONESHOT flag as set. */
        let events = ev.flags | EPOLLERR | EPOLLHUP;

        self.user_evts = events;
        self.user_data = ev.data;

        (events & KNOWN_EPOLL_EVENTS & !self.pending_evts) != 0
    }

    fn update(&mut self, self_ptr: *const Mutex<Sock>) -> io::Result<()> {
        assert!(!self.delete_pending);

        if self.poll_status == SockPollStatus::Pending
            && (self.user_evts & KNOWN_EPOLL_EVENTS & !self.pending_evts) == 0
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
            self.poll_info.handles[0].events = eventflags_to_afd_events(self.user_evts);

            let apccontext = unsafe { mem::transmute(self_ptr) };
            let result = self
                .afd
                .poll(&mut self.poll_info, &mut self.iosb.0, apccontext);
            if let Err(e) = result {
                if let Some(code) = e.raw_os_error() {
                    if code == ERROR_IO_PENDING as i32 {
                        /* Overlapped poll operation in progress; this is expected. */
                    } else if code == ERROR_INVALID_HANDLE as i32 {
                        /* Socket closed; it'll be dropped from the epoll set. */
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
        self.afd.cancel(&mut self.iosb.0)?;
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
        let mut epoll_events = 0;
        self.poll_status = SockPollStatus::Idle;
        self.pending_evts = 0;

        unsafe {
            if self.delete_pending {
                return None;
            } else if self.iosb.0.u.Status == STATUS_CANCELLED {
                /* The poll request was cancelled by CancelIoEx. */
            } else if !NT_SUCCESS(self.iosb.0.u.Status) {
                /* The overlapped request itself failed in an unexpected way. */
                epoll_events &= EPOLLERR;
            } else if self.poll_info.number_of_handles < 1 {
                /* This poll operation succeeded but didn't report any socket events. */
            } else if self.poll_info.handles[0].events & super::afd::AFD_POLL_LOCAL_CLOSE != 0 {
                self.mark_delete();
                return None;
            } else {
                epoll_events =
                    super::afd::afd_events_to_eventflags(self.poll_info.handles[0].events);
            }
        }

        epoll_events &= self.user_evts;

        if epoll_events == 0 {
            return None;
        }

        if (self.user_evts & EPOLLONESHOT) != 0 {
            self.user_evts = 0;
        }

        // Codes to emulate ET in mio
        if (epoll_events & EPOLLIN) != 0 {
            self.user_evts &= !EPOLLIN;
        }
        if (epoll_events & EPOLLOUT) != 0 {
            self.user_evts &= !EPOLLOUT;
        }

        Some(Event {
            data: self.user_data,
            flags: epoll_events,
        })
    }

    fn is_pending_deletion(&self) -> bool {
        self.delete_pending
    }

    fn raw_socket(&self) -> RawSocket {
        self.raw_socket
    }

    fn poll_status(&self) -> SockPollStatus {
        self.poll_status
    }
}

/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associated with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

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
                id,
                inner: Arc::new(inner),
            }
        })
    }
    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.select(events, timeout)
    }

    pub fn register<T: AsRawSocket>(
        &self,
        socket: &T,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.inner
            .register(socket.as_raw_socket(), token, interests)
    }

    pub fn reregister<T: AsRawSocket>(
        &self,
        socket: &T,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.inner
            .reregister(socket.as_raw_socket(), token, interests)
    }

    pub fn deregister<T: AsRawSocket>(&self, socket: &T) -> io::Result<()> {
        self.inner.deregister(socket.as_raw_socket())
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub(super) fn clone_inner(&self) -> Arc<SelectorInner> {
        self.inner.clone()
    }
}

#[derive(Debug)]
pub struct SelectorInner {
    cp: Arc<CompletionPort>,
    active_poll_count: AtomicUsize,
    update_queue: Mutex<VecDeque<RawSocket>>,
    deleted_queue: Mutex<VecDeque<Arc<Mutex<Sock>>>>,
    afd_group: Mutex<Vec<Arc<Afd>>>,
    socket_map: Mutex<HashMap<RawSocket, Arc<Mutex<Sock>>>>,
}

impl SelectorInner {
    pub fn new() -> io::Result<SelectorInner> {
        CompletionPort::new(0).map(|cp| SelectorInner {
            cp: Arc::new(cp),
            active_poll_count: AtomicUsize::new(0),
            update_queue: Mutex::new(VecDeque::new()),
            deleted_queue: Mutex::new(VecDeque::new()),
            afd_group: Mutex::new(Vec::new()),
            socket_map: Mutex::new(HashMap::new()),
        })
    }

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();

        self.update_sockets_events()?;

        self.active_poll_count.fetch_add(1, Ordering::SeqCst);

        let mut iocp_events = vec![CompletionStatus::zero(); events.capacity()];
        let result = self.cp.get_many(&mut iocp_events, timeout);

        self.active_poll_count.fetch_sub(1, Ordering::SeqCst);

        if let Err(e) = result {
            use winapi::shared::winerror::WAIT_TIMEOUT;
            if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) {
                return Ok(());
            }
            return Err(e);
        }

        self.feed_events(events, result.unwrap());
        Ok(())
    }

    pub fn register(
        &self,
        raw_socket: RawSocket,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        let flags = interests_to_epoll(interests);

        self._alloc_sock_for_rawsocket_only_if_not_existed(raw_socket)?;

        let event = Event {
            flags,
            data: token.0 as u64,
        };
        self._set_socket_event(raw_socket, event);
        self.update_sockets_events_if_polling()?;

        Ok(())
    }

    pub fn reregister(
        &self,
        raw_socket: RawSocket,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        let flags = interests_to_epoll(interests);

        {
            let socket_map = self.socket_map.lock().unwrap();
            match socket_map.get(&raw_socket) {
                Some(_) => {}
                None => return Err(io::Error::from(io::ErrorKind::NotFound)),
            }
        }

        let event = Event {
            flags,
            data: token.0 as u64,
        };
        self._set_socket_event(raw_socket, event);
        self.update_sockets_events_if_polling()?;

        Ok(())
    }

    pub fn deregister(&self, raw_socket: RawSocket) -> io::Result<()> {
        {
            let mut socket_map = self.socket_map.lock().unwrap();
            match socket_map.get_mut(&raw_socket) {
                Some(sock) => {
                    let mut sock_internal = sock.lock().unwrap();
                    sock_internal.mark_delete();
                }
                None => return Err(io::Error::from(io::ErrorKind::NotFound)),
            }
        }
        self._cleanup_deleted_sock();
        self._release_unused_afd();
        Ok(())
    }

    pub fn port(&self) -> Arc<CompletionPort> {
        self.cp.clone()
    }

    fn update_sockets_events(&self) -> io::Result<()> {
        {
            let mut update_queue = self.update_queue.lock().unwrap();
            let mut socket_map = self.socket_map.lock().unwrap();
            loop {
                let rawsock = match update_queue.pop_front() {
                    Some(rawsock) => rawsock,
                    None => break,
                };
                match socket_map.get_mut(&rawsock) {
                    Some(sock) => {
                        let ptr = Arc::into_raw(sock.clone());
                        let mut sock_internal = sock.lock().unwrap();
                        if !sock_internal.is_pending_deletion() {
                            sock_internal.update(ptr).unwrap();
                        }
                    }
                    None => {}
                }
            }
        }
        self._cleanup_deleted_sock();
        self._release_unused_afd();
        Ok(())
    }

    fn update_sockets_events_if_polling(&self) -> io::Result<()> {
        if self.active_poll_count.load(Ordering::SeqCst) > 0 {
            return self.update_sockets_events();
        }
        Ok(())
    }

    fn feed_events(&self, events: &mut Events, iocp_events: &[CompletionStatus]) {
        {
            let mut update_queue = self.update_queue.lock().unwrap();
            for iocp_event in iocp_events.iter() {
                if iocp_event.overlapped() as usize == 0 {
                    events.push(Event {
                        flags: EPOLLIN,
                        data: iocp_event.token() as u64,
                    });
                    continue;
                }
                let sock: Arc<Mutex<Sock>> =
                    unsafe { Arc::from_raw(mem::transmute(iocp_event.overlapped())) };
                let mut sock_guard = sock.lock().unwrap();
                match sock_guard.feed_event() {
                    Some(e) => {
                        events.push(e);
                    }
                    None => {}
                }
                if !sock_guard.is_pending_deletion() {
                    update_queue.push_back(sock_guard.raw_socket());
                }
            }
        }
        self._cleanup_deleted_sock();
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

    fn _alloc_sock_for_rawsocket_only_if_not_existed(
        &self,
        raw_socket: RawSocket,
    ) -> io::Result<()> {
        let mut socket_map = self.socket_map.lock().unwrap();
        match socket_map.get(&raw_socket) {
            Some(_) => return Err(io::Error::from(io::ErrorKind::AlreadyExists)),
            None => {}
        };
        let sock = Arc::new(Mutex::new(Sock::new(raw_socket, self._acquire_afd()?)?));
        socket_map.insert(raw_socket, sock);
        Ok(())
    }

    fn _set_socket_event(&self, raw_socket: RawSocket, event: Event) {
        let mut socket_map = self.socket_map.lock().unwrap();

        if socket_map
            .get_mut(&raw_socket)
            .unwrap()
            .lock()
            .unwrap()
            .set_event(event)
        {
            let mut update_queue = self.update_queue.lock().unwrap();
            update_queue.push_back(raw_socket);
        }
    }

    fn _cleanup_deleted_sock(&self) {
        let mut socket_map = self.socket_map.lock().unwrap();

        socket_map.iter().for_each(|(_, sock)| {
            let sock_internal = sock.lock().unwrap();
            if !sock_internal.is_pending_deletion() {
                return;
            }
            if sock_internal.poll_status() != SockPollStatus::Idle {
                let mut deleted_queue = self.deleted_queue.lock().unwrap();
                deleted_queue.push_back(sock.clone());
            }
        });
        socket_map.retain(|_, sock| !sock.lock().unwrap().is_pending_deletion());
    }
}

pub const EPOLLIN: u32 = (1 << 0);
pub const EPOLLPRI: u32 = (1 << 1);
pub const EPOLLOUT: u32 = (1 << 2);
pub const EPOLLERR: u32 = (1 << 3);
pub const EPOLLHUP: u32 = (1 << 4);
pub const EPOLLRDNORM: u32 = (1 << 6);
pub const EPOLLRDBAND: u32 = (1 << 7);
pub const EPOLLWRNORM: u32 = (1 << 8);
pub const EPOLLWRBAND: u32 = (1 << 9);
pub const EPOLLMSG: u32 = (1 << 10); /* Never reported. */
pub const EPOLLRDHUP: u32 = (1 << 13);
pub const EPOLLONESHOT: u32 = (1 << 31);

pub const KNOWN_EPOLL_EVENTS: u32 = EPOLLIN
    | EPOLLPRI
    | EPOLLOUT
    | EPOLLERR
    | EPOLLHUP
    | EPOLLRDNORM
    | EPOLLRDBAND
    | EPOLLWRNORM
    | EPOLLWRBAND
    | EPOLLMSG
    | EPOLLRDHUP;

fn interests_to_epoll(interests: Interests) -> u32 {
    let mut kind = 0;

    if interests.is_readable() {
        kind |= EPOLLIN;
    }

    if interests.is_writable() {
        kind |= EPOLLOUT;
    }

    kind
}

/*
fn epoll_to_interests(epoll_flags: u32) -> Option<Interests> {
    if epoll_flags | EPOLLIN != 0 && epoll_flags | EPOLLOUT != 0 {
        Some(Interests::READABLE | Interests::WRITABLE)
    } else if epoll_flags | EPOLLIN != 0 {
        Some(Interests::READABLE)
    } else if epoll_flags | EPOLLOUT != 0 {
        Some(Interests::WRITABLE)
    } else {
        None
    }
}
*/

fn get_base_socket(raw_socket: RawSocket) -> io::Result<RawSocket> {
    let mut base_socket: RawSocket = 0;
    let mut bytes: u32 = 0;
    const SIO_BASE_HANDLE: u32 = 0x48000022;

    use std::mem::{size_of, transmute};
    use std::ptr::null_mut;
    use winapi::um::winsock2::{WSAIoctl, SOCKET_ERROR};

    unsafe {
        if WSAIoctl(
            raw_socket as usize,
            SIO_BASE_HANDLE,
            null_mut(),
            0,
            transmute(&mut base_socket),
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
