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
        try!(self.selector.register(io.desc(), token.as_uint(), interest, opts));

        Ok(())
    }

    pub fn reregister<H: IoHandle>(&mut self, io: &H, token: Token, interest: event::Interest, opts: event::PollOpt) -> MioResult<()> {
        debug!("registering  with poller");

        // Register interests for this socket
        try!(self.selector.reregister(io.desc(), token.as_uint(), interest, opts));

        Ok(())
    }

    pub fn poll(&mut self, timeout_ms: uint) -> MioResult<uint> {
        try!(self.selector.select(&mut self.events, timeout_ms));
        Ok(self.events.len())
    }

    pub fn event(&self, idx: uint) -> event::IoEvent {
        self.events.get(idx)
    }
}
