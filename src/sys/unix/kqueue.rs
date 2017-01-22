use std::{cmp, fmt};
use std::cell::RefCell;
use std::os::unix::io::RawFd;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use std::time::Duration;

use libc::{self, time_t};

use {io, Ready, PollOpt, Token};
use event::{self, Event};
use sys::unix::cvt;
use sys::unix::io::set_cloexec;

/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associated with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
static NEXT_ID: AtomicUsize = ATOMIC_USIZE_INIT;

pub struct Selector {
    id: usize,
    kq: RawFd,
    changes: RefCell<KeventList>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        // offset by 1 to avoid choosing 0 as the id of a selector
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;
        let kq = unsafe { try!(cvt(libc::kqueue())) };
        drop(set_cloexec(kq));

        Ok(Selector {
            id: id,
            kq: kq,
            changes: RefCell::new(KeventList(Vec::new())),
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn select(&self, evts: &mut Events, awakener: Token, timeout: Option<Duration>) -> io::Result<bool> {
        let timeout = timeout.map(|to| {
            libc::timespec {
                tv_sec: cmp::min(to.as_secs(), time_t::max_value() as u64) as time_t,
                tv_nsec: to.subsec_nanos() as libc::c_long,
            }
        });
        let timeout = timeout.as_ref().map(|s| s as *const _).unwrap_or(0 as *const _);

        unsafe {
            let cnt = try!(cvt(libc::kevent(self.kq,
                                            0 as *const _,
                                            0,
                                            evts.sys_events.0.as_mut_ptr(),
                                            evts.sys_events.0.capacity() as i32,
                                            timeout)));

            self.changes.borrow_mut().0.clear();
            evts.sys_events.0.set_len(cnt as usize);

            Ok(evts.coalesce(awakener))
        }
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: Ready, opts: PollOpt) -> io::Result<()> {
        trace!("registering; token={:?}; interests={:?}", token, interests);

        self.ev_register(fd,
                         token.into(),
                         libc::EVFILT_READ,
                         interests.contains(Ready::readable()),
                         opts);
        self.ev_register(fd,
                         token.into(),
                         libc::EVFILT_WRITE,
                         interests.contains(Ready::writable()),
                         opts);

        self.flush_changes()
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: Ready, opts: PollOpt) -> io::Result<()> {
        // Just need to call register here since EV_ADD is a mod if already
        // registered
        self.register(fd, token, interests, opts)
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        self.ev_push(fd, 0, libc::EVFILT_READ, libc::EV_DELETE);
        self.ev_push(fd, 0, libc::EVFILT_WRITE, libc::EV_DELETE);

        self.flush_changes()
    }

    fn ev_register(&self,
                   fd: RawFd,
                   token: usize,
                   filter: i16,
                   enable: bool,
                   opts: PollOpt) {
        let mut flags = libc::EV_ADD;

        if enable {
            flags = flags | libc::EV_ENABLE;
        } else {
            flags = flags | libc::EV_DISABLE;
        }

        if opts.contains(PollOpt::edge()) {
            flags = flags | libc::EV_CLEAR;
        }

        if opts.contains(PollOpt::oneshot()) {
            flags = flags | libc::EV_ONESHOT;
        }

        self.ev_push(fd, token, filter, flags);
    }

    fn ev_push(&self,
               fd: RawFd,
               token: usize,
               filter: i16,
               flags: u16) {
        self.changes.borrow_mut().0.push(libc::kevent {
            ident: fd as ::libc::uintptr_t,
            filter: filter,
            flags: flags,
            fflags: 0,
            data: 0,
            udata: token as *mut _,
        });
    }

    fn flush_changes(&self) -> io::Result<()> {
        unsafe {
            let mut changes = self.changes.borrow_mut();
            try!(cvt(libc::kevent(self.kq,
                                  changes.0.as_mut_ptr() as *const _,
                                  changes.0.len() as i32,
                                  0 as *mut _,
                                  0,
                                  0 as *const _)));
            changes.0.clear();
            Ok(())
        }
    }
}

impl fmt::Debug for Selector {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Selector")
            .field("id", &self.id)
            .field("kq", &self.kq)
            .field("changes", &self.changes.borrow().0.len())
            .finish()
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::close(self.kq);
        }
    }
}

pub struct Events {
    sys_events: KeventList,
    events: Vec<Event>,
    event_map: HashMap<Token, usize>,
}

struct KeventList(Vec<libc::kevent>);

unsafe impl Send for KeventList {}
unsafe impl Sync for KeventList {}

impl Events {
    pub fn with_capacity(cap: usize) -> Events {
        Events {
            sys_events: KeventList(Vec::with_capacity(cap)),
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

        for e in self.sys_events.0.iter() {
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
                self.events.push(Event::new(Ready::none(), token));

            }

            if e.flags & libc::EV_ERROR != 0 {
                event::kind_mut(&mut self.events[idx]).insert(Ready::error());
            }

            if e.filter == libc::EVFILT_READ {
                event::kind_mut(&mut self.events[idx]).insert(Ready::readable());
            } else if e.filter == libc::EVFILT_WRITE {
                event::kind_mut(&mut self.events[idx]).insert(Ready::writable());
            }

            if e.flags & libc::EV_EOF != 0 {
                event::kind_mut(&mut self.events[idx]).insert(Ready::hup());

                // When the read end of the socket is closed, EV_EOF is set on
                // flags, and fflags contains the error if there is one.
                if e.fflags != 0 {
                    event::kind_mut(&mut self.events[idx]).insert(Ready::error());
                }
            }
        }

        ret
    }

    pub fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }
}

impl fmt::Debug for Events {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Events {{ len: {} }}", self.sys_events.0.len())
    }
}
