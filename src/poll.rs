use {Evented, Token, MioResult};
use os::{self, event};
use std::fmt;

pub struct Poll {
    selector: os::Selector,
    events: os::Events
}

impl Poll {
    pub fn new() -> MioResult<Poll> {
        Ok(Poll {
            selector: try!(os::Selector::new()),
            events: os::Events::new()
        })
    }

    pub fn register<E: Evented>(&mut self, io: &E, token: Token, interest: event::Interest, opts: event::PollOpt) -> MioResult<()> {
        debug!("registering  with poller");

        // Register interests for this socket
        try!(self.selector.register(io.as_raw_fd(), token.as_usize(), interest, opts));

        Ok(())
    }

    pub fn reregister<E: Evented>(&mut self, io: &E, token: Token, interest: event::Interest, opts: event::PollOpt) -> MioResult<()> {
        debug!("registering  with poller");

        // Register interests for this socket
        try!(self.selector.reregister(io.as_raw_fd(), token.as_usize(), interest, opts));

        Ok(())
    }

    pub fn deregister<E: Evented>(&mut self, io: &E) -> MioResult<()> {
        debug!("deregistering IO with poller");

        // Deregister interests for this socket
        try!(self.selector.deregister(io.as_raw_fd()));

        Ok(())
    }

    pub fn poll(&mut self, timeout_ms: usize) -> MioResult<usize> {
        try!(self.selector.select(&mut self.events, timeout_ms));
        Ok(self.events.len())
    }

    pub fn event(&self, idx: usize) -> event::IoEvent {
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
    events: &'a os::Events,
    index: usize
}

impl<'a> Iterator for EventsIterator<'a> {
    type Item = event::IoEvent;

    fn next(&mut self) -> Option<event::IoEvent> {
        if self.index == self.events.len() {
            None
        } else {
            self.index += 1;
            Some(self.events.get(self.index - 1))
        }
    }
}
