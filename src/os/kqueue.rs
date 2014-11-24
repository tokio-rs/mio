use std::mem;
use nix::fcntl::Fd;
use nix::sys::event::*;
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
        if interests.contains(event::READABLE) {
            try!(self.ev_push(io, EVFILT_READ, token, opts));
        }

        if interests.contains(event::WRITABLE) {
            try!(self.ev_push(io, EVFILT_WRITE, token, opts));
        }

        Ok(())
    }

    pub fn reregister(&mut self, io: &IoDesc, token: uint, interests: Interest, opts: PollOpt) -> MioResult<()> {
        // Just need to call register here since EV_ADD is a mod if already
        // registered
        self.register(io, token, interests, opts)
    }

    // Queues an event change. Events will get submitted to the OS on the next
    // call to select or when the change buffer fills up.
    //
    // EV_ADD       Add a filter or modify
    // EV_DELETE    remove a filter
    // EV_ONESHOT   one shot behavior
    // EV_CLEAR     clear event after returning
    fn ev_push(&mut self, io: &IoDesc, filter: EventFilter, token: uint, opts: PollOpt) -> MioResult<()> {
        try!(self.maybe_flush_changes());

        let idx = self.changes.len;
        let ev = &mut self.changes.events[idx];

        let mut flags = EV_ADD;

        if opts.contains(event::EDGE) {
            flags = flags | EV_CLEAR;
        }

        if opts.contains(event::ONESHOT) {
            flags = flags | EV_ONESHOT;
        }

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
