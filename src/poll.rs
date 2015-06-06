use {sys, Evented, Token};
use event::{Interest, PollOpt};
use std::{fmt, io};

pub use sys::{Events, EventsIterator };

pub struct Poll {
    selector: sys::Selector,
    events: Option<sys::Events>
}

impl Poll {
    pub fn new() -> io::Result<Poll> {
        Ok(Poll {
            selector: try!(sys::Selector::new()),
            events: Some(sys::Events::new())
        })
    }

    pub fn register<E: ?Sized>(&mut self, io: &E, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        trace!("registering with poller");

        // Register interests for this socket
        try!(io.register(&mut self.selector, token, interest, opts));

        Ok(())
    }

    pub fn reregister<E: ?Sized>(&mut self, io: &E, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        trace!("registering with poller");

        // Register interests for this socket
        try!(io.reregister(&mut self.selector, token, interest, opts));

        Ok(())
    }

    pub fn deregister<E: ?Sized>(&mut self, io: &E) -> io::Result<()>
        where E: Evented
    {
        trace!("deregistering IO with poller");

        // Deregister interests for this socket
        try!(io.deregister(&mut self.selector));

        Ok(())
    }

    pub fn poll(&mut self, timeout_ms: usize) -> io::Result<Events> {
        let mut evts = self.events.take().expect("poll run without events struct set. Call set_events");
        try!(self.selector.select(&mut evts, timeout_ms));
        evts.coalesce();
        Ok(evts)
    }

    pub fn reset_events(&mut self, evts: Events) {
        self.events = Some(evts)
    }
}

impl fmt::Debug for Poll {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Poll")
    }
}

