use {sys, Evented, Token};
use event::{Interest, IoEvent, PollOpt};
use std::{fmt, io};

pub struct Poll {
    selector: sys::Selector,
    events: sys::Events
}

impl Poll {
    pub fn new() -> io::Result<Poll> {
        Ok(Poll {
            selector: try!(sys::Selector::new()),
            events: sys::Events::new()
        })
    }

    pub fn register<E: Evented>(&mut self, io: &E, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        debug!("registering  with poller");

        // Register interests for this socket
        try!(io.register(&mut self.selector, token, interest, opts));

        Ok(())
    }

    pub fn reregister<E: Evented>(&mut self, io: &E, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        debug!("registering  with poller");

        // Register interests for this socket
        try!(io.reregister(&mut self.selector, token, interest, opts));

        Ok(())
    }

    pub fn deregister<E: Evented>(&mut self, io: &E) -> io::Result<()> {
        debug!("deregistering IO with poller");

        // Deregister interests for this socket
        try!(io.deregister(&mut self.selector));

        Ok(())
    }

    pub fn poll(&mut self, timeout_ms: usize) -> io::Result<usize> {
        try!(self.selector.select(&mut self.events, timeout_ms));
        Ok(self.events.len())
    }

    pub fn event(&self, idx: usize) -> IoEvent {
        self.events.get(idx)
    }

    pub fn iter(&self) -> EventsIterator {
        EventsIterator { events: &self.events, index: 0 }
    }
}

impl fmt::Debug for Poll {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Poll")
    }
}

pub struct EventsIterator<'a> {
    events: &'a sys::Events,
    index: usize
}

impl<'a> Iterator for EventsIterator<'a> {
    type Item = IoEvent;

    fn next(&mut self) -> Option<IoEvent> {
        if self.index == self.events.len() {
            None
        } else {
            self.index += 1;
            Some(self.events.get(self.index - 1))
        }
    }
}
