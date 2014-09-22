use std::mem;
use nix::fcntl::Fd;
use nix::sys::event::*;
use error::{MioResult, MioError};
use poll::{IoEvent, IoReadable, IoWritable, IoError};

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

    pub fn register(&mut self, fd: Fd, token: uint) -> MioResult<()> {
        let flag = EV_ADD | EV_CLEAR;

        try!(self.ev_push(fd, EVFILT_READ,  flag, FilterFlag::empty(), token));
        try!(self.ev_push(fd, EVFILT_WRITE, flag, FilterFlag::empty(), token));

        Ok(())
    }

    pub fn unregister(&mut self, fd: Fd) -> MioResult<()> {
        let flag = EV_DELETE;

        try!(self.ev_push(fd, EVFILT_READ,  flag, FilterFlag::empty(), 0));
        try!(self.ev_push(fd, EVFILT_WRITE, flag, FilterFlag::empty(), 0));

        Ok(())
    }

    // Queues an event change. Events will get submitted to the OS on the next
    // call to select or when the change buffer fills up.
    fn ev_push(&mut self,
               fd: Fd,
               filter: EventFilter,
               flags: EventFlag,
               fflags: FilterFlag,
               token: uint) -> MioResult<()> {

        // If the change buffer is full, flush it
        try!(self.maybe_flush_changes());

        let idx = self.changes.len;
        let ev = &mut self.changes.events[idx];

        ev_set(ev, fd as uint, filter, flags, fflags, token);

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
            fail!("invalid index");
        }

        let ev = &self.events[idx];
        let token = ev.udata;

        // When the read end of the socket is closed, EV_EOF is set on the
        // flags, and fflags contains the error, if any.

        let mut kind;

        if ev.filter == EVFILT_READ {
            kind = IoReadable;
        } else if ev.filter == EVFILT_WRITE {
            kind = IoWritable;
        } else {
            unimplemented!();
        }

        if ev.flags.contains(EV_EOF) {
            // When the read end of the socket is closed, EV_EOF is set on
            // flags, and fflags contains the error if there is one.
            if !ev.fflags.is_empty() {
                kind = kind | IoError;
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
