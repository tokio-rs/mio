use {io, Interest, PollOpt, Token};
use event::IoEvent;
use nix::sys::event::{EventFilter, EventFlag, FilterFlag, KEvent, ev_set, kqueue, kevent};
use nix::sys::event::{EV_ADD, EV_CLEAR, EV_DELETE, EV_DISABLE, EV_ENABLE, EV_EOF, EV_ONESHOT};
use std::slice;
use std::os::unix::io::RawFd;

pub struct Selector {
    kq: RawFd,
    changes: Events
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        Ok(Selector {
            kq: try!(kqueue().map_err(super::from_nix_error)),
            changes: Events::new()
        })
    }

    pub fn select(&mut self, evts: &mut Events, timeout_ms: usize) -> io::Result<()> {
        let cnt = try!(kevent(self.kq, self.changes.as_slice(),
                              evts.as_mut_slice(), timeout_ms)
                                  .map_err(super::from_nix_error));

        self.changes.events.clear();

        unsafe {
            evts.events.set_len(cnt);
        }

        Ok(())
    }

    pub fn register(&mut self, fd: RawFd, token: Token, interests: Interest, opts: PollOpt) -> io::Result<()> {
        debug!("registering; token={:?}; interests={:?}", token, interests);

        try!(self.ev_register(fd, token.as_usize(), EventFilter::EVFILT_READ, interests.contains(Interest::readable()), opts));
        try!(self.ev_register(fd, token.as_usize(), EventFilter::EVFILT_WRITE, interests.contains(Interest::writable()), opts));

        Ok(())
    }

    pub fn reregister(&mut self, fd: RawFd, token: Token, interests: Interest, opts: PollOpt) -> io::Result<()> {
        // Just need to call register here since EV_ADD is a mod if already
        // registered
        self.register(fd, token, interests, opts)
    }

    pub fn deregister(&mut self, fd: RawFd) -> io::Result<()> {
        try!(self.ev_push(fd, 0, EventFilter::EVFILT_READ, EV_DELETE));
        try!(self.ev_push(fd, 0, EventFilter::EVFILT_WRITE, EV_DELETE));

        Ok(())
    }

    fn ev_register(&mut self, fd: RawFd, token: usize, filter: EventFilter, enable: bool, opts: PollOpt) -> io::Result<()> {
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

        self.ev_push(fd, token, filter, flags)
    }

    fn ev_push(&mut self, fd: RawFd, token: usize, filter: EventFilter, flags: EventFlag) -> io::Result<()> {
        try!(self.maybe_flush_changes());

        let idx = self.changes.len();

        // Increase the vec size
        unsafe { self.changes.events.set_len(idx + 1) };

        let ev = &mut self.changes.events[idx];

        ev_set(ev, fd as usize, filter, flags, FilterFlag::empty(), token);

        Ok(())
    }

    fn maybe_flush_changes(&mut self) -> io::Result<()> {
        if self.changes.is_full() {
            try!(kevent(self.kq, self.changes.as_slice(), &mut [], 0)
                    .map_err(super::from_nix_error));

            self.changes.events.clear();
        }

        Ok(())
    }
}

pub struct Events {
    events: Vec<KEvent>,
}

impl Events {
    pub fn new() -> Events {
        Events { events: Vec::with_capacity(1024) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    // TODO: We will get rid of this eventually in favor of an iterator
    #[inline]
    pub fn get(&self, idx: usize) -> IoEvent {
        if idx >= self.len() {
            panic!("invalid index");
        }

        let ev = &self.events[idx];
        let token = ev.udata;

        debug!("get event; token={}; ev.filter={:?}; ev.flags={:?}", token, ev.filter, ev.flags);

        // When the read end of the socket is closed, EV_EOF is set on the
        // flags, and fflags contains the error, if any.

        let mut kind = Interest::hinted();

        if ev.filter == EventFilter::EVFILT_READ {
            kind = kind | Interest::readable();
        } else if ev.filter == EventFilter::EVFILT_WRITE {
            kind = kind | Interest::writable();
        } else {
            panic!("GOT: {:?}", ev.filter);
        }

        if ev.flags.contains(EV_EOF) {
            kind = kind | Interest::hup();

            // When the read end of the socket is closed, EV_EOF is set on
            // flags, and fflags contains the error if there is one.
            if !ev.fflags.is_empty() {
                kind = kind | Interest::error();
            }
        }

        IoEvent::new(kind, token)
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.len() == self.events.capacity()
    }

    fn as_slice(&self) -> &[KEvent] {
        unsafe {
            let ptr = (&self.events[..]).as_ptr();
            slice::from_raw_parts(ptr, self.events.len())
        }
    }

    fn as_mut_slice(&mut self) -> &mut [KEvent] {
        unsafe {
            let ptr = (&mut self.events[..]).as_mut_ptr();
            slice::from_raw_parts_mut(ptr, self.events.capacity())
        }
    }
}
