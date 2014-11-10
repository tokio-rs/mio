use error::MioResult;
use io::IoHandle;
use os;
use event_ctx::*;

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

    pub fn register<H: IoHandle>(&mut self, io: &H, eventctx: &IoEventCtx) -> MioResult<()> {
        debug!("registering IO with poller");

        // Register interests for this socket
        try!(self.selector.register(io.desc(), eventctx));

        Ok(())
    }
    
    pub fn reregister<H: IoHandle>(&mut self, io: &H, eventctx: &IoEventCtx) -> MioResult<()> {
        debug!("registering IO with poller");

        // Register interests for this socket
        try!(self.selector.reregister(io.desc(), eventctx));

        Ok(())
    }

    pub fn poll(&mut self, timeout_ms: uint) -> MioResult<uint> {
        try!(self.selector.select(&mut self.events, timeout_ms));
        Ok(self.events.len())
    }

    pub fn event(&self, idx: uint) -> IoEventCtx {
        self.events.get(idx)
    }
}


