use {Listenable, Interest, PollOpt, Token};
use event::Event;
use os::{Selector, Events};
use std::fmt;
use std::io::Result;

pub struct Poll {
    selector: Selector,
    events: Events
}

impl Poll {
    pub fn new() -> Result<Poll> {
        Ok(Poll {
            selector: try!(Selector::new()),
            events: Events::new()
        })
    }

    pub fn register<L: Listenable>(&mut self, io: &L, token: Token, interest: Interest, opts: PollOpt) -> Result<()> {
        debug!("registering  with poller");

        // Register interests for this socket
        try!(self.selector.register(io.desc(), token.as_usize(), interest, opts));

        Ok(())
    }

    pub fn reregister<L: Listenable>(&mut self, io: &L, token: Token, interest: Interest, opts: PollOpt) -> Result<()> {
        debug!("registering  with poller");

        // Register interests for this socket
        try!(self.selector.reregister(io.desc(), token.as_usize(), interest, opts));

        Ok(())
    }

    pub fn deregister<L: Listenable>(&mut self, io: &L) -> Result<()> {
        debug!("deregistering IO with poller");

        // Deregister interests for this socket
        try!(self.selector.deregister(io.desc()));

        Ok(())
    }

    pub fn poll(&mut self, timeout_ms: usize) -> Result<usize> {
        try!(self.selector.select(&mut self.events, timeout_ms));
        Ok(self.events.len())
    }

    pub fn event(&self, idx: usize) -> Event {
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
    events: &'a Events,
    index: usize
}

impl<'a> Iterator for EventsIterator<'a> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        if self.index == self.events.len() {
            None
        } else {
            self.index += 1;
            Some(self.events.get(self.index - 1))
        }
    }
}
