use {io, EventSet, PollOpt, Token};
use event::IoEvent;
use nix::sys::event::{EventFilter, EventFlag, FilterFlag, KEvent, kqueue, kevent, kevent_ts};
use nix::sys::event::{EV_ADD, EV_CLEAR, EV_DELETE, EV_DISABLE, EV_ENABLE, EV_EOF, EV_ERROR, EV_ONESHOT};
use libc::{timespec, time_t, c_long};
use std::{fmt, slice};
use std::os::unix::io::RawFd;
use std::collections::HashMap;
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
    kq: RawFd,
    changes: Events
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let kq = try!(kqueue().map_err(super::from_nix_error));

        Ok(Selector {
            id: id,
            kq: kq,
            changes: Events::new()
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn select(&mut self, evts: &mut Events, timeout_ms: Option<usize>) -> io::Result<()> {
        let timeout = timeout_ms.map(|x| timespec {
            tv_sec: (x / 1000) as time_t,
            tv_nsec: ((x % 1000) * 1_000_000) as c_long
        });

        let cnt = try!(kevent_ts(self.kq, &[], evts.as_mut_slice(), timeout)
                                  .map_err(super::from_nix_error));

        self.changes.sys_events.clear();

        unsafe {
            evts.sys_events.set_len(cnt);
        }

        evts.coalesce();

        Ok(())
    }

    pub fn register(&mut self, fd: RawFd, token: Token, interests: EventSet, opts: PollOpt) -> io::Result<()> {
        trace!("registering; token={:?}; interests={:?}", token, interests);

        self.ev_register(fd, token.as_usize(), EventFilter::EVFILT_READ, interests.contains(EventSet::readable()), opts);
        self.ev_register(fd, token.as_usize(), EventFilter::EVFILT_WRITE, interests.contains(EventSet::writable()), opts);

        self.flush_changes()
    }

    pub fn reregister(&mut self, fd: RawFd, token: Token, interests: EventSet, opts: PollOpt) -> io::Result<()> {
        // Just need to call register here since EV_ADD is a mod if already
        // registered
        self.register(fd, token, interests, opts)
    }

    pub fn deregister(&mut self, fd: RawFd) -> io::Result<()> {
        self.ev_push(fd, 0, EventFilter::EVFILT_READ, EV_DELETE);
        self.ev_push(fd, 0, EventFilter::EVFILT_WRITE, EV_DELETE);

        self.flush_changes()
    }

    fn ev_register(&mut self, fd: RawFd, token: usize, filter: EventFilter, enable: bool, opts: PollOpt) {
        let mut flags = EV_ADD;

        if enable {
            flags = flags | EV_ENABLE;
        } else {
            flags = flags | EV_DISABLE;
        }

        if opts.contains(PollOpt::edge()) {
            flags = flags | EV_CLEAR;
        }

        if opts.contains(PollOpt::oneshot()) {
            flags = flags | EV_ONESHOT;
        }

        self.ev_push(fd, token, filter, flags);
    }

    #[cfg(not(target_os = "netbsd"))]
    fn ev_push(&mut self, fd: RawFd, token: usize, filter: EventFilter, flags: EventFlag) {
        self.changes.sys_events.push(
            KEvent {
                ident: fd as ::libc::uintptr_t,
                filter: filter,
                flags: flags,
                fflags: FilterFlag::empty(),
                data: 0,
                udata: token
            });
    }

    #[cfg(target_os = "netbsd")]
    fn ev_push(&mut self, fd: RawFd, token: usize, filter: EventFilter, flags: EventFlag) {
        self.changes.sys_events.push(
            KEvent {
                ident: fd as ::libc::uintptr_t,
                filter: filter,
                flags: flags,
                fflags: FilterFlag::empty(),
                data: 0,
                udata: token as i64
            });
    }

    fn flush_changes(&mut self) -> io::Result<()> {
        let result = kevent(self.kq, self.changes.as_slice(), &mut [], 0).map(|_| ())
            .map_err(super::from_nix_error).map(|_| ());

        self.changes.sys_events.clear();
        result
    }
}

pub struct Events {
    sys_events: Vec<KEvent>,
    events: Vec<IoEvent>,
    event_map: HashMap<Token, usize>,
}

impl Events {
    pub fn new() -> Events {
        Events {
            sys_events: Vec::with_capacity(1024),
            events: Vec::with_capacity(1024),
            event_map: HashMap::with_capacity(1024)
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn get(&self, idx: usize) -> IoEvent {
        self.events[idx]
    }

    pub fn coalesce(&mut self) {
        self.events.clear();
        self.event_map.clear();

        for e in self.sys_events.iter() {
            let token = Token(e.udata as usize);
            let len = self.events.len();

            let idx = *self.event_map.entry(token)
                .or_insert(len);

            if idx == len {
                // New entry, insert the default
                self.events.push(IoEvent::new(EventSet::none(), token));

            }

            if e.flags.contains(EV_ERROR) {
                self.events[idx].kind.insert(EventSet::error());
            }

            if e.filter == EventFilter::EVFILT_READ {
                self.events[idx].kind.insert(EventSet::readable());
            } else if e.filter == EventFilter::EVFILT_WRITE {
                self.events[idx].kind.insert(EventSet::writable());
            }

            if e.flags.contains(EV_EOF) {
                self.events[idx].kind.insert(EventSet::hup());

                // When the read end of the socket is closed, EV_EOF is set on
                // flags, and fflags contains the error if there is one.
                if !e.fflags.is_empty() {
                    self.events[idx].kind.insert(EventSet::error());
                }
            }
        }
    }

    fn as_slice(&self) -> &[KEvent] {
        unsafe {
            let ptr = (&self.sys_events[..]).as_ptr();
            slice::from_raw_parts(ptr, self.sys_events.len())
        }
    }

    fn as_mut_slice(&mut self) -> &mut [KEvent] {
        unsafe {
            let ptr = (&mut self.sys_events[..]).as_mut_ptr();
            slice::from_raw_parts_mut(ptr, self.sys_events.capacity())
        }
    }
}

impl fmt::Debug for Events {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Events {{ len: {} }}", self.sys_events.len())
    }
}
