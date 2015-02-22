use std::mem;
use nix::fcntl::Fd;
// use nix::sys::epoll::*;
use nix::unistd::close;
use error::{MioResult, MioError};
use os::IoDesc;
use os::event::{IoEvent, Interest, PollOpt};

pub struct Selector {
    epfd: Fd
}

impl Selector {
    pub fn new() -> MioResult<Selector> {
        let epfd = try!(epoll_create().map_err(MioError::from_nix_error));

        Ok(Selector { epfd: epfd })
    }

    /// Wait for events from the OS
    pub fn select(&mut self, evts: &mut Events, timeout_ms: usize) -> MioResult<()> {
        // Wait for epoll events for at most timeout_ms milliseconds
        let cnt = try!(epoll_wait(self.epfd, evts.events.as_mut_slice(), timeout_ms)
                           .map_err(MioError::from_nix_error));

        evts.len = cnt;
        Ok(())
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&mut self, io: &IoDesc, token: usize, interests: Interest, opts: PollOpt) -> MioResult<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(interests, opts),
            data: token as u64
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlAdd, io.fd, &info)
            .map_err(MioError::from_nix_error)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn reregister(&mut self, io: &IoDesc, token: usize, interests: Interest, opts: PollOpt) -> MioResult<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(interests, opts),
            data: token as u64
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlMod, io.fd, &info)
            .map_err(MioError::from_nix_error)
    }

    /// Deregister event interests for the given IO handle with the OS
    pub fn deregister(&mut self, io: &IoDesc) -> MioResult<()> {
        // The &info argument should be ignored by the system,
        // but linux < 2.6.9 required it to be not null.
        // For compatibility, we provide a dummy EpollEvent.
        let info = EpollEvent {
            events: EpollEventKind::empty(),
            data: 0
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlDel, io.fd, &info)
            .map_err(MioError::from_nix_error)
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
    len: usize,
    events: [EpollEvent; 1024]
}

impl Events {
    pub fn new() -> Events {
        Events {
            len: 0,
            events: unsafe { mem::uninitialized() }
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn get(&self, idx: usize) -> IoEvent {
        if idx >= self.len {
            panic!("invalid index");
        }

        let epoll = self.events[idx].events;
        let mut kind = Interest::hinted();

        if epoll.contains(EPOLLIN) {
            kind = kind | Interest::readable();
        }

        if epoll.contains(EPOLLOUT) {
            kind = kind | Interest::writable();
        }

        // EPOLLHUP - Usually means a socket error happened
        if epoll.contains(EPOLLERR) {
            kind = kind | Interest::error();
        }

        if epoll.contains(EPOLLRDHUP) | epoll.contains(EPOLLHUP) {
            kind = kind | Interest::hup();
        }

        let token = self.events[idx].data;

        IoEvent::new(kind, token as usize)
    }
}
