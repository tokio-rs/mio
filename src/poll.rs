use {convert, sys, Evented, Token};
use event::{EventSet, Event, PollOpt};
use std::{fmt, io};
use std::time::Duration;

/// The `Poll` type acts as an interface allowing a program to wait on a set of
/// IO handles until one or more become "ready" to be operated on. An IO handle
/// is considered ready to operate on when the given operation can complete
/// without blocking.
///
/// To use `Poll`, an IO handle must first be registered with the `Poll`
/// instance using the `register()` handle. An `EventSet` representing the
/// program's interest in the socket is specified as well as an arbitrary
/// `Token` which is used to identify the IO handle in the future.
///
/// ## Edge-triggered and level-triggered
///
/// An IO handle registration may request edge-triggered notifications or
/// level-triggered notifications. This is done by specifying the `PollOpt`
/// argument to `register()` and `reregister()`.
///
/// ## Portability
///
/// Cross platform portability is provided for Mio's TCP & UDP implementations.
///
/// ## Examples
///
/// ```no_run
/// use mio::*;
/// use mio::tcp::*;
///
/// // Construct a new `Poll` handle
/// let mut poll = Poll::new().unwrap();
///
/// // Connect the stream
/// let stream = TcpStream::connect(&"173.194.33.80:80".parse().unwrap()).unwrap();
///
/// // Register the stream with `Poll`
/// poll.register(&stream, Token(0), EventSet::all(), PollOpt::edge()).unwrap();
///
/// // Wait for the socket to become ready
/// poll.poll(None).unwrap();
/// ```
pub struct Poll {
    selector: sys::Selector,
    events: sys::Events,
}

impl Poll {
    pub fn new() -> io::Result<Poll> {
        Ok(Poll {
            selector: try!(sys::Selector::new()),
            events: sys::Events::new(),
        })
    }

    pub fn register<E: ?Sized>(&mut self, io: &E, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        trace!("registering with poller");

        // Register interests for this socket
        try!(io.register(self, token, interest, opts));

        Ok(())
    }

    pub fn reregister<E: ?Sized>(&mut self, io: &E, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        trace!("registering with poller");

        // Register interests for this socket
        try!(io.reregister(self, token, interest, opts));

        Ok(())
    }

    pub fn deregister<E: ?Sized>(&mut self, io: &E) -> io::Result<()>
        where E: Evented
    {
        trace!("deregistering IO with poller");

        // Deregister interests for this socket
        try!(io.deregister(self));

        Ok(())
    }

    pub fn poll(&mut self, timeout: Option<Duration>) -> io::Result<usize> {
        let timeout = timeout.map(|to| convert::millis(to) as usize);
        try!(self.selector.select(&mut self.events, timeout));
        Ok(self.events.len())
    }

    pub fn events(&self) -> Events {
        Events {
            curr: 0,
            poll: self,
        }
    }
}

impl fmt::Debug for Poll {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Poll")
    }
}

pub struct Events<'a> {
    curr: usize,
    poll: &'a Poll,
}

impl<'a> Events<'a> {
    pub fn get(&self, idx: usize) -> Option<Event> {
        self.poll.events.get(idx)
    }

    pub fn len(&self) -> usize {
        self.poll.events.len()
    }
}

impl<'a> Iterator for Events<'a> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        if self.curr == self.poll.events.len() {
            return None;
        }

        let ret = self.poll.events.get(self.curr).unwrap();
        self.curr += 1;
        Some(ret)
    }
}

// ===== Accessors for internal usage =====

pub fn selector(poll: &Poll) -> &sys::Selector {
    &poll.selector
}

pub fn selector_mut(poll: &mut Poll) -> &mut sys::Selector {
    &mut poll.selector
}
