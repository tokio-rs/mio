use std::fmt;
use error::MioResult;
use io::IoHandle;
use os;
use os::token::Token;
use os::event;

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

    pub fn register<H: IoHandle>(&mut self, io: &H, token: Token, interest: event::Interest, opts: event::PollOpt) -> MioResult<()> {
        debug!("registering  with poller");

        // Register interests for this socket
        try!(self.selector.register(io.fd(), token.as_usize(), interest, opts));

        Ok(())
    }

    pub fn reregister<H: IoHandle>(&mut self, io: &H, token: Token, interest: event::Interest, opts: event::PollOpt) -> MioResult<()> {
        debug!("registering  with poller");

        // Register interests for this socket
        try!(self.selector.reregister(io.fd(), token.as_usize(), interest, opts));

        Ok(())
    }

    pub fn deregister<H: IoHandle>(&mut self, io: &H) -> MioResult<()> {
        debug!("deregistering IO with poller");

        // Deregister interests for this socket
        try!(self.selector.deregister(io.fd()));

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
