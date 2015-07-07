use {io, Interest, PollOpt, Token};
use event::IoEvent;
use nix::sys::event::{EventFilter, EventFlag, FilterFlag, KEvent, kqueue, kevent};
use nix::sys::event::{EV_ADD, EV_CLEAR, EV_DELETE, EV_DISABLE, EV_ENABLE, EV_EOF, EV_ONESHOT};
use std::{fmt, slice};
use std::os::unix::io::RawFd;
use std::collections::HashMap;
use std::collections::hash_map::Values;

#[derive(Debug)]
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

        evts.coalesce();

        Ok(())
    }

    pub fn register(&mut self, fd: RawFd, token: Token, interests: Interest, opts: PollOpt) -> io::Result<()> {
        trace!("registering; token={:?}; interests={:?}", token, interests);

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

        self.changes.events.push(
            KEvent {
                ident: fd as ::libc::uintptr_t,
                filter: filter,
                flags: flags,
                fflags: FilterFlag::empty(),
                data: 0,
                udata: token
            });

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
    token_evts: HashMap<Token, IoEvent>
}

impl Events {
    pub fn new() -> Events {
        Events { events: Vec::with_capacity(1024),
                 token_evts: HashMap::with_capacity(1024) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.token_evts.len()
    }

    pub fn coalesce(&mut self) {
        self.token_evts.clear();
        for e in self.events.iter() {
            let ioe = self.token_evts.entry(Token(e.udata as usize)).or_insert(IoEvent::new(Interest::hinted(), e.udata as usize));
            if e.filter == EventFilter::EVFILT_READ {
                ioe.kind.insert(Interest::readable());
            } else if e.filter == EventFilter::EVFILT_WRITE {
                ioe.kind.insert(Interest::writable());
            }

            if e.flags.contains(EV_EOF) {
                ioe.kind.insert(Interest::hup());

                // When the read end of the socket is closed, EV_EOF is set on
                // flags, and fflags contains the error if there is one.
                if !e.fflags.is_empty() {
                    ioe.kind.insert(Interest::error());
                }
            }
        }
    }

    pub fn iter<'a>(&'a self) -> EventsIterator<'a> {
        EventsIterator{ iter: self.token_evts.values() }
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

impl fmt::Debug for Events {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Events {{ len: {} }}", self.events.len())
    }
}

pub struct EventsIterator<'a> {
    iter: Values<'a, Token, IoEvent>
}

impl<'a> Iterator for EventsIterator<'a> {
    type Item = IoEvent;

    fn next(&mut self) -> Option<IoEvent> {
        self.iter.next().map(|x| *x)
    }
}
