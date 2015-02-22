use {io, Interest, PollOpt};
use event::Event;
use nix::sys::event::*;
use nix::sys::event::EventFilter::*;
use std::mem;
use std::io::Result;
use std::os::unix::Fd;

pub struct Selector {
    kq: Fd,
    changes: Events
}

impl Selector {
    pub fn new() -> Result<Selector> {
        Ok(Selector {
            kq: try!(kqueue().map_err(io::to_io_error)),
            changes: Events::new()
        })
    }

    pub fn select(&mut self, evts: &mut Events, timeout_ms: usize) -> Result<()> {
        let cnt = try!(kevent(self.kq, self.changes.as_slice(),
                              evts.as_mut_slice(), timeout_ms)
                                  .map_err(io::to_io_error));

        self.changes.len = 0;

        evts.len = cnt;
        Ok(())
    }

    pub fn register(&mut self, fd: Fd, token: usize, interests: Interest, opts: PollOpt) -> Result<()> {
        debug!("registering; token={}; interests={:?}", token, interests);

        try!(self.ev_register(fd, token, EVFILT_READ, interests.contains(Interest::readable()), opts));
        try!(self.ev_register(fd, token, EVFILT_WRITE, interests.contains(Interest::writable()), opts));

        Ok(())
    }

    pub fn reregister(&mut self, fd: Fd, token: usize, interests: Interest, opts: PollOpt) -> Result<()> {
        // Just need to call register here since EV_ADD is a mod if already
        // registered
        self.register(fd, token, interests, opts)
    }

    pub fn deregister(&mut self, fd: Fd) -> Result<()> {
        try!(self.ev_push(fd, 0, EVFILT_READ, EV_DELETE));
        try!(self.ev_push(fd, 0, EVFILT_WRITE, EV_DELETE));

        Ok(())
    }

    fn ev_register(&mut self, fd: Fd, token: usize, filter: EventFilter, enable: bool, opts: PollOpt) -> Result<()> {
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

    fn ev_push(&mut self, fd: Fd, token: usize, filter: EventFilter, flags: EventFlag) -> Result<()> {
        try!(self.maybe_flush_changes());

        let idx = self.changes.len;
        let ev = &mut self.changes.events[idx];

        ev_set(ev, fd as usize, filter, flags, FilterFlag::empty(), token);

        self.changes.len += 1;
        Ok(())
    }

    fn maybe_flush_changes(&mut self) -> Result<()> {
        if self.changes.is_full() {
            try!(kevent(self.kq, self.changes.as_slice(), &mut [], 0)
                    .map_err(io::to_io_error));
            self.changes.len = 0;
        }

        Ok(())
    }
}

pub struct Events {
    len: usize,
    events: [KEvent; 1024]
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

    // TODO: We will get rid of this eventually in favor of an iterator
    #[inline]
    pub fn get(&self, idx: usize) -> Event {
        if idx >= self.len {
            panic!("invalid index");
        }

        let ev = &self.events[idx];
        let token = ev.udata;

        debug!("get event; token={}; ev.filter={:?}; ev.flags={:?}", token, ev.filter, ev.flags);

        // When the read end of the socket is closed, EV_EOF is set on the
        // flags, and fflags contains the error, if any.

        let mut kind = Interest::hinted();

        if ev.filter == EVFILT_READ {
            kind = kind | Interest::readable();
        } else if ev.filter == EVFILT_WRITE {
            kind = kind | Interest::writable();
        } else {
            unimplemented!();
        }

        if ev.flags.contains(EV_EOF) {
            kind = kind | Interest::hup();

            // When the read end of the socket is closed, EV_EOF is set on
            // flags, and fflags contains the error if there is one.
            if !ev.fflags.is_empty() {
                kind = kind | Interest::error();
            }
        }

        Event::new(kind, token)
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.len == self.events.len()
    }

    fn as_slice(&self) -> &[KEvent] {
        &self.events[..self.len]
    }

    fn as_mut_slice(&mut self) -> &mut [KEvent] {
        self.events.as_mut_slice()
    }
}
