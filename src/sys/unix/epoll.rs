use {io, Interest, PollOpt, Token};
use event::IoEvent;
use nix::sys::epoll::*;
use nix::unistd::close;
use std::os::unix::io::RawFd;
use std::slice::Iter;

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

        evts.coalesce();

        Ok(())
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&mut self, fd: RawFd, token: Token, interests: Interest, opts: PollOpt) -> io::Result<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(interests, opts),
            data: token.as_usize() as u64
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlAdd, fd, &info)
            .map_err(super::from_nix_error)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn reregister(&mut self, fd: RawFd, token: Token, interests: Interest, opts: PollOpt) -> io::Result<()> {
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

fn ioevent_to_epoll(interest: Interest, opts: PollOpt) -> EpollEventKind {
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
    token_evts: Vec<IoEvent>
}

impl Events {
    pub fn new() -> Events {
        Events { events: Vec::with_capacity(1024),
                 token_evts: Vec::with_capacity(1024)
               }
    }

    #[inline]
    pub fn coalesce(&mut self) {

        unsafe { self.token_evts.set_len(self.events.len()); }

        for (i,e) in self.events.iter().enumerate() {
            let mut kind = Interest::hinted();

            if e.events.contains(EPOLLIN) {
                kind = kind | Interest::readable();
            }

            if e.events.contains(EPOLLOUT) {
                kind = kind | Interest::writable();
            }

            // EPOLLHUP - Usually means a socket error happened
            if e.events.contains(EPOLLERR) {
                kind = kind | Interest::error();
            }

            if e.events.contains(EPOLLRDHUP) | e.events.contains(EPOLLHUP) {
                kind = kind | Interest::hup();
            }

            self.token_evts[i] = IoEvent::new(kind, e.data as usize);
        }
    }

    #[inline]
    pub fn iter<'a>(&'a self) -> EventsIterator<'a> {
        EventsIterator{ iter: self.token_evts.iter() }
    }
}

pub struct EventsIterator<'a> {
    iter: Iter<'a, IoEvent>
}

impl<'a> Iterator for EventsIterator<'a> {
    type Item = IoEvent;

    fn next(&mut self) -> Option<IoEvent> {
        self.iter.next().map(|e| *e)
    }
}
