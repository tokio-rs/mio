use {io, EventSet, PollOpt, Token};
use event::IoEvent;
use nix::sys::epoll::*;
use nix::unistd::close;
use std::os::unix::io::RawFd;

#[derive(Debug)]
pub struct Selector {
    epfd: RawFd
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let epfd = try!(epoll_create().map_err(super::from_nix_error));

        Ok(Selector { epfd: epfd })
    }

    /// Wait for events from the OS
    pub fn select(&mut self, evts: &mut Events, timeout_ms: usize) -> io::Result<()> {
        use std::{isize, slice};

        let timeout_ms = if timeout_ms >= isize::MAX as usize {
            isize::MAX
        } else {
            timeout_ms as isize
        };

        let dst = unsafe {
            slice::from_raw_parts_mut(
                evts.events.as_mut_ptr(),
                evts.events.capacity())
        };

        // Wait for epoll events for at most timeout_ms milliseconds
        let cnt = try!(epoll_wait(self.epfd, dst, timeout_ms)
                           .map_err(super::from_nix_error));

        unsafe { evts.events.set_len(cnt); }

        Ok(())
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&mut self, fd: RawFd, token: Token, interests: EventSet, opts: PollOpt) -> io::Result<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(interests, opts),
            data: token.as_usize() as u64
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlAdd, fd, &info)
            .map_err(super::from_nix_error)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn reregister(&mut self, fd: RawFd, token: Token, interests: EventSet, opts: PollOpt) -> io::Result<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(interests, opts),
            data: token.as_usize() as u64
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlMod, fd, &info)
            .map_err(super::from_nix_error)
    }

    /// Deregister event interests for the given IO handle with the OS
    pub fn deregister(&mut self, fd: RawFd) -> io::Result<()> {
        // The &info argument should be ignored by the system,
        // but linux < 2.6.9 required it to be not null.
        // For compatibility, we provide a dummy EpollEvent.
        let info = EpollEvent {
            events: EpollEventKind::empty(),
            data: 0
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlDel, fd, &info)
            .map_err(super::from_nix_error)
    }
}

fn ioevent_to_epoll(interest: EventSet, opts: PollOpt) -> EpollEventKind {
    let mut kind = EpollEventKind::empty();

    if interest.is_readable() {
        kind.insert(EPOLLIN);
    }

    if interest.is_writable() {
        kind.insert(EPOLLOUT);
    }

    if interest.is_hup() {
        kind.insert(EPOLLRDHUP);
    }

    if opts.is_edge() {
        kind.insert(EPOLLET);
    }

    if opts.is_oneshot() {
        kind.insert(EPOLLONESHOT);
    }

    if opts.is_level() {
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
    events: Vec<EpollEvent>,
}

impl Events {
    pub fn new() -> Events {
        Events {
            events: Vec::with_capacity(1024),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[inline]
    pub fn get(&self, idx: usize) -> IoEvent {
        let epoll = self.events[idx].events;
        let mut kind = EventSet::hinted();

        if epoll.contains(EPOLLIN) {
            kind = kind | EventSet::readable();
        }

        if epoll.contains(EPOLLOUT) {
            kind = kind | EventSet::writable();
        }

        // EPOLLHUP - Usually means a socket error happened
        if epoll.contains(EPOLLERR) {
            kind = kind | EventSet::error();
        }

        if epoll.contains(EPOLLRDHUP) | epoll.contains(EPOLLHUP) {
            kind = kind | EventSet::hup();
        }

        let token = self.events[idx].data;

        IoEvent::new(kind, Token(token as usize))
    }
}
