use std::mem;
use nix::fcntl::Fd;
use nix::sys::epoll::*;
use nix::unistd::close;
use error::{MioResult, MioError};
use os;
use event_ctx::*;
use token::Token;

pub struct Selector {
    epfd: Fd
}

impl Selector {
    pub fn new() -> MioResult<Selector> {
        let epfd = try!(epoll_create().map_err(MioError::from_sys_error));

        Ok(Selector { epfd: epfd })
    }

    /// Wait for events from the OS
    pub fn select(&mut self, evts: &mut Events, timeout_ms: uint) -> MioResult<()> {
        // Wait for epoll events for at most timeout_ms milliseconds
        let cnt = try!(epoll_wait(self.epfd, evts.events.as_mut_slice(), timeout_ms)
                           .map_err(MioError::from_sys_error));

        evts.len = cnt;
        Ok(())
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&mut self, io: &os::IoDesc, eventctx: &IoEventCtx) -> MioResult<()> {
        let evt = io_to_epoll_event(eventctx);
        epoll_ctl(self.epfd, EpollCtlAdd, io.fd, &evt).map_err(MioError::from_sys_error)
    }

    /// Re-Register event interests for the given IO handle with the OS
    pub fn reregister(&mut self, io: &os::IoDesc, eventctx: &IoEventCtx) -> MioResult<()> {
        let evt = io_to_epoll_event(eventctx);
        epoll_ctl(self.epfd, EpollCtlMod, io.fd, &evt).map_err(MioError::from_sys_error)
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = close(self.epfd);
    }
}

/// convert on IoEventCtx event set into an Epoll Event set
/// with the purpose of setting up the registrar with the
/// proper edge triggered or level triggered behavior
/// If IOEDGE is passed into the event set, EPOLLONESHOT 
/// is automatically added so that the event must be re-registered
/// at the end of every poll.wait()
/// If IOEDGE is not passed, then epoll defaults to level triggered
fn io_to_epoll_event(evt: &IoEventCtx) -> EpollEvent {

    let mut interests = EpollEventKind::empty(); 

    if evt.is_readable() {
        interests = interests | EPOLLIN;
    }
    if evt.is_writable() {
        interests = interests | EPOLLOUT;
    }
    if evt.is_error() {
        interests = interests | EPOLLERR;
    }
    if evt.is_hangup() {
        interests = interests | EPOLLHUP | EPOLLRDHUP;
    }
    if evt.is_edge_triggered() {
        interests = interests | EPOLLET | EPOLLONESHOT;
    }

    EpollEvent {
        events: interests,
        data: evt.token().as_uint() as u64 
    }
}

pub struct Events {
    len: uint,
    events: [EpollEvent, ..1024]
}

impl Events {
    pub fn new() -> Events {
        Events {
            len: 0,
            events: unsafe { mem::uninitialized() }
        }
    }

    #[inline]
    pub fn len(&self) -> uint {
        self.len
    }

    #[inline]
    pub fn get(&self, idx: uint) -> IoEventCtx {
        if idx >= self.len {
            panic!("invalid index");
        }

        let epoll = self.events[idx].events;
        let mut kind = IoEventKind::empty() | IOHINTED;

        debug!("epoll events: {}", epoll);
        if epoll.contains(EPOLLIN) {
            kind = kind | IOREADABLE;
        }

        if epoll.contains(EPOLLOUT) {
            kind = kind | IOWRITABLE;
        }

        // EPOLLHUP - Usually means a socket error happened
        if epoll.contains(EPOLLERR) {
            kind = kind | IOERROR;
        }

        if epoll.contains(EPOLLRDHUP) || epoll.contains(EPOLLHUP) {
            kind = kind | IOHUPHINT;
        }

        let token = self.events[idx].data;

        IoEventCtx::new(kind, Token(token as uint))
    }
}
