use {io, EventSet, PollOpt, Token};
use event::{self, Event};
use nix::libc::timespec;
use nix::unistd::close;
use nix::sys::event::{EventFilter, EventFlag, FilterFlag, KEvent, kqueue, kevent, kevent_ts};
use nix::sys::event::{EV_ADD, EV_CLEAR, EV_DELETE, EV_DISABLE, EV_ENABLE, EV_EOF, EV_ERROR, EV_ONESHOT};
use libc::time_t;
use std::{cmp, fmt, slice};
use std::cell::UnsafeCell;
use std::os::unix::io::RawFd;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use std::time::Duration;

/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associatd with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
static NEXT_ID: AtomicUsize = ATOMIC_USIZE_INIT;

pub struct Selector {
    id: usize,
    kq: RawFd,
    changes: UnsafeCell<Vec<KEvent>>,
}

#[cfg(not(target_os = "netbsd"))]
type UData = usize;

#[cfg(target_os = "netbsd")]
type UData = isize;

impl Selector {
    pub fn new() -> io::Result<Selector> {
        // offset by 1 to avoid choosing 0 as the id of a selector
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;
        let kq = try!(kqueue().map_err(super::from_nix_error));

        Ok(Selector {
            id: id,
            kq: kq,
            changes: UnsafeCell::new(vec![]),
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn select(&self, evts: &mut Events, awakener: Token, timeout: Option<Duration>) -> io::Result<bool> {
        let timeout = timeout.map(|to| timespec {
            tv_sec: cmp::min(to.as_secs(), time_t::max_value() as u64) as time_t,
            tv_nsec: to.subsec_nanos() as i64,
        });

        let cnt = try!(kevent_ts(self.kq, &[], evts.as_mut_slice(), timeout)
                                  .map_err(super::from_nix_error));

        self.mut_changes().clear();

        unsafe {
            evts.sys_events.set_len(cnt);
        }

        Ok(evts.coalesce(awakener))
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: EventSet, opts: PollOpt) -> io::Result<()> {
        trace!("registering; token={:?}; interests={:?}", token, interests);

        self.ev_register(fd, usize::from(token), EventFilter::EVFILT_READ, interests.contains(EventSet::readable()), opts);
        self.ev_register(fd, usize::from(token), EventFilter::EVFILT_WRITE, interests.contains(EventSet::writable()), opts);

        self.flush_changes()
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: EventSet, opts: PollOpt) -> io::Result<()> {
        // Just need to call register here since EV_ADD is a mod if already
        // registered
        self.register(fd, token, interests, opts)
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        self.ev_push(fd, 0, EventFilter::EVFILT_READ, EV_DELETE);
        self.ev_push(fd, 0, EventFilter::EVFILT_WRITE, EV_DELETE);

        self.flush_changes()
    }

    fn ev_register(&self, fd: RawFd, token: usize, filter: EventFilter, enable: bool, opts: PollOpt) {
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

    fn ev_push(&self, fd: RawFd, token: usize, filter: EventFilter, flags: EventFlag) {
        self.mut_changes().push(
            KEvent {
                ident: fd as ::libc::uintptr_t,
                filter: filter,
                flags: flags,
                fflags: FilterFlag::empty(),
                data: 0,
                udata: token as UData,
            });
    }

    fn flush_changes(&self) -> io::Result<()> {
        let result = kevent(self.kq, self.changes(), &mut [], 0).map(|_| ())
            .map_err(super::from_nix_error).map(|_| ());

        self.mut_changes().clear();
        result
    }

    fn changes(&self) -> &[KEvent] {
        unsafe { &(*self.changes.get())[..] }
    }

    fn mut_changes(&self) -> &mut Vec<KEvent> {
        unsafe { &mut *self.changes.get() }
    }
}

impl fmt::Debug for Selector {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Selector")
            .field("id", &self.id)
            .field("kq", &self.kq)
            .field("changes", &self.changes().len())
            .finish()
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = close(self.kq);
    }
}

pub struct Events {
    sys_events: Vec<KEvent>,
    events: Vec<Event>,
    event_map: HashMap<Token, usize>,
}

impl Events {
    pub fn with_capacity(cap: usize) -> Events {
        Events {
            sys_events: Vec::with_capacity(cap),
            events: Vec::with_capacity(cap),
            event_map: HashMap::with_capacity(cap)
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

    pub fn get(&self, idx: usize) -> Option<Event> {
        self.events.get(idx).map(|e| *e)
    }

    fn coalesce(&mut self, awakener: Token) -> bool {
        let mut ret = false;
        self.events.clear();
        self.event_map.clear();

        for e in &self.sys_events {
            let token = Token(e.udata as usize);
            let len = self.events.len();

            if token == awakener {
                // TODO: Should this return an error if event is an error. It
                // is not critical as spurious wakeups are permitted.
                ret = true;
                continue;
            }

            let idx = *self.event_map.entry(token)
                .or_insert(len);

            if idx == len {
                // New entry, insert the default
                self.events.push(Event::new(EventSet::none(), token));

            }

            if e.flags.contains(EV_ERROR) {
                event::kind_mut(&mut self.events[idx]).insert(EventSet::error());
            }

            if e.filter == EventFilter::EVFILT_READ {
                event::kind_mut(&mut self.events[idx]).insert(EventSet::readable());
            } else if e.filter == EventFilter::EVFILT_WRITE {
                event::kind_mut(&mut self.events[idx]).insert(EventSet::writable());
            }

            if e.flags.contains(EV_EOF) {
                event::kind_mut(&mut self.events[idx]).insert(EventSet::hup());

                // When the read end of the socket is closed, EV_EOF is set on
                // flags, and fflags contains the error if there is one.
                if !e.fflags.is_empty() {
                    event::kind_mut(&mut self.events[idx]).insert(EventSet::error());
                }
            }
        }

        ret
    }

    pub fn push_event(&mut self, event: Event) {
        self.events.push(event);
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
