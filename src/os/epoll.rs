use std::mem;
use nix::fcntl::Fd;
use nix::sys::epoll::*;
use nix::unistd::close;
use error::{MioResult, MioError};
use os::IoDesc;
use os::event;

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
    pub fn register(&mut self, io: &IoDesc, token: uint, interests: event::Interest, opts: event::PollOpt) -> MioResult<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(interests, opts),
            data: token as u64
        };

        epoll_ctl(self.epfd, EpollCtlAdd, io.fd, &info)
            .map_err(MioError::from_sys_error)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn reregister(&mut self, io: &IoDesc, token: uint, interests: event::Interest, opts: event::PollOpt) -> MioResult<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(interests, opts),
            data: token as u64
        };

        epoll_ctl(self.epfd, EpollCtlMod, io.fd, &info)
            .map_err(MioError::from_sys_error)
    }
}

fn ioevent_to_epoll(interest: event::Interest, opts: event::PollOpt) -> EpollEventKind {
    let mut kind = EpollEventKind::empty();

    if interest.contains(event::READABLE) {
        kind.insert(EPOLLIN);
    }

    if interest.contains(event::WRITABLE) {
        kind.insert(EPOLLOUT);
    }

    if interest.contains(event::HUP) {
        kind.insert(EPOLLRDHUP);
    }

    if opts.contains(event::EDGE) {
        kind.insert(EPOLLET);
    }

    if opts.contains(event::ONESHOT) {
        kind.insert(EPOLLONESHOT);
    }

    if opts.contains(event::LEVEL) {
        kind.remove(EPOLLET);
    }

    kind
}

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = close(self.epfd);
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
    pub fn get(&self, idx: uint) -> event::IoEvent {
        if idx >= self.len {
            panic!("invalid index");
        }

        let epoll = self.events[idx].events;
        let mut kind = event::Interest::empty() | event::HINTED;

        if epoll.contains(EPOLLIN) {
            kind = kind | event::READABLE;
        }

        if epoll.contains(EPOLLOUT) {
            kind = kind | event::WRITABLE;
        }

        // EPOLLHUP - Usually means a socket error happened
        if epoll.contains(EPOLLERR) {
            kind = kind | event::ERROR;
        }

        if epoll.contains(EPOLLRDHUP) | epoll.contains(EPOLLHUP) {
            kind = kind | event::HUP;
        }

        let token = self.events[idx].data;

        event::IoEvent::new(kind, token as uint)
    }
}
