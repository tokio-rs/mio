use std::mem;
use nix::fcntl::Fd;
use nix::sys::event::*;
use nix::sys::event::EventFilter::*;
use error::{MioResult, MioError};
use os::IoDesc;
use os::event;
use os::event::{IoEvent, Interest, PollOpt};

pub struct Selector {
    kq: Fd,
    changes: Events
}

impl Selector {
    pub fn new() -> MioResult<Selector> {
        Ok(Selector {
            kq: try!(kqueue().map_err(MioError::from_sys_error)),
            changes: Events::new()
        })
    }

    pub fn select(&mut self, evts: &mut Events, timeout_ms: uint) -> MioResult<()> {
        let cnt = try!(kevent(self.kq, self.changes.as_slice(),
                              evts.as_mut_slice(), timeout_ms)
                                  .map_err(MioError::from_sys_error));

        self.changes.len = 0;

        evts.len = cnt;
        Ok(())
    }

    pub fn register(&mut self, io: &IoDesc, token: uint, interests: Interest, opts: PollOpt) -> MioResult<()> {
        debug!("registering; token={}; interests={}", token, interests);

        try!(self.ev_register(io, token, EVFILT_READ, interests.contains(event::READABLE), opts));
        try!(self.ev_register(io, token, EVFILT_WRITE, interests.contains(event::WRITABLE), opts));

        Ok(())
    }

    pub fn reregister(&mut self, io: &IoDesc, token: uint, interests: Interest, opts: PollOpt) -> MioResult<()> {
        // Just need to call register here since EV_ADD is a mod if already
        // registered
        self.register(io, token, interests, opts)
    }

    pub fn deregister(&mut self, io: &IoDesc) -> MioResult<()> {
        try!(self.ev_push(io, 0, EVFILT_READ, EV_DELETE));
        try!(self.ev_push(io, 0, EVFILT_WRITE, EV_DELETE));

        Ok(())
    }

    fn ev_register(&mut self, io: &IoDesc, token: uint, filter: EventFilter, enable: bool, opts: PollOpt) -> MioResult<()> {
        let mut flags = EV_ADD;

        if enable {
            flags = flags | EV_ENABLE;
        } else {
            flags = flags | EV_DISABLE;
        }

        if opts.contains(event::EDGE) {
            flags = flags | EV_CLEAR;
        }

        if opts.contains(event::ONESHOT) {
            flags = flags | EV_ONESHOT;
        }

        self.ev_push(io, token, filter, flags)
    }

    fn ev_push(&mut self, io: &IoDesc, token: uint, filter: EventFilter, flags: EventFlag) -> MioResult<()> {
        try!(self.maybe_flush_changes());

        let idx = self.changes.len;
        let ev = &mut self.changes.events[idx];

        ev_set(ev, io.fd as uint, filter, flags, FilterFlag::empty(), token);

        self.changes.len += 1;
        Ok(())
    }

    fn maybe_flush_changes(&mut self) -> MioResult<()> {
        if self.changes.is_full() {
            try!(kevent(self.kq, self.changes.as_slice(), &mut [], 0)
                    .map_err(MioError::from_sys_error));
            self.changes.len = 0;
        }

        Ok(())
    }
}

pub struct Events {
    len: uint,
    events: [KEvent, ..1024]
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

    // TODO: We will get rid of this eventually in favor of an iterator
    #[inline]
    pub fn get(&self, idx: uint) -> IoEvent {
        if idx >= self.len {
            panic!("invalid index");
        }

        let ev = &self.events[idx];
        let token = ev.udata;

        debug!("get event; token={}; ev.filter={}; ev.flags={}", token, ev.filter, ev.flags);

        // When the read end of the socket is closed, EV_EOF is set on the
        // flags, and fflags contains the error, if any.

        let mut kind = event::HINTED;

        if ev.filter == EVFILT_READ {
            kind = kind | event::READABLE;
        } else if ev.filter == EVFILT_WRITE {
            kind = kind | event::WRITABLE;
        } else {
            unimplemented!();
        }

        if ev.flags.contains(EV_EOF) {
            kind = kind | event::HUP;

            // When the read end of the socket is closed, EV_EOF is set on
            // flags, and fflags contains the error if there is one.
            if !ev.fflags.is_empty() {
                kind = kind | event::ERROR;
            }
        }

        IoEvent::new(kind, token)
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.len == self.events.len()
    }

    fn as_slice(&self) -> &[KEvent] {
        self.events.slice_to(self.len)
    }

    fn as_mut_slice(&mut self) -> &mut [KEvent] {
        self.events.as_mut_slice()
    }
}
