use {io, EventSet, PollOpt, Token};
use event::Event;
use nix::sys::epoll::*;
use nix::unistd::close;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};

/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associatd with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
static NEXT_ID: AtomicUsize = ATOMIC_USIZE_INIT;

#[derive(Debug)]
pub struct Selector {
    id: usize,
    epfd: RawFd
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        // offset by 1 to avoid choosing 0 as the id of a selector
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;
        let epfd = try!(epoll_create().map_err(super::from_nix_error));

        Ok(Selector {
            id: id,
            epfd: epfd,
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Wait for events from the OS
    pub fn select(&self, evts: &mut Events, awakener: Token, timeout_ms: Option<usize>) -> io::Result<bool> {
        use std::{cmp, i32, slice};

        let timeout_ms = match timeout_ms {
            None => -1 as i32,
            Some(x) => cmp::min(i32::MAX as usize, x) as i32,
        };

        let dst = unsafe {
            slice::from_raw_parts_mut(
                evts.events.as_mut_ptr(),
                evts.events.capacity())
        };

        // Wait for epoll events for at most timeout_ms milliseconds
        let cnt = try!(epoll_wait(self.epfd, dst, timeout_ms as isize)
                           .map_err(super::from_nix_error));

        unsafe { evts.events.set_len(cnt); }

        for i in 0..cnt {
            if evts.get(i).map(|e| e.token()) == Some(awakener) {
                evts.events.remove(i);
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&self, fd: RawFd, token: Token, interests: EventSet, opts: PollOpt) -> io::Result<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(interests, opts),
            data: usize::from(token) as u64
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlAdd, fd, &info)
            .map_err(super::from_nix_error)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn reregister(&self, fd: RawFd, token: Token, interests: EventSet, opts: PollOpt) -> io::Result<()> {
        let info = EpollEvent {
            events: ioevent_to_epoll(interests, opts),
            data: usize::from(token) as u64
        };

        epoll_ctl(self.epfd, EpollOp::EpollCtlMod, fd, &info)
            .map_err(super::from_nix_error)
    }

    /// Deregister event interests for the given IO handle with the OS
    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
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
        if opts.is_urgent() {
            kind.insert(EPOLLPRI);
        } else {
            kind.insert(EPOLLIN);
        }
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
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Option<Event> {
        self.events.get(idx).map(|event| {
            let epoll = event.events;
            let mut kind = EventSet::none();

            if epoll.contains(EPOLLIN) |  epoll.contains(EPOLLPRI) {
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

            Event::new(kind, Token(token as usize))
        })
    }

    pub fn push_event(&mut self, event: Event) {
        self.events.push(EpollEvent {
            events: ioevent_to_epoll(event.kind(), PollOpt::empty()),
            data: usize::from(event.token()) as u64
        });
    }
}
