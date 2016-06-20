use std::sync::{Mutex, MutexGuard};

use {io, poll, Evented, EventSet, Poll, PollOpt, Token};
use sys::windows::selector::Registration;
use miow::iocp::CompletionStatus;

pub struct Awakener {
    iocp: Mutex<Registration>,
}

impl Awakener {
    pub fn new() -> io::Result<Awakener> {
        Ok(Awakener {
            iocp: Mutex::new(Registration::new()),
        })
    }

    pub fn wakeup(&self) -> io::Result<()> {
        // Each wakeup notification has NULL as its `OVERLAPPED` pointer to
        // indicate that it's from this awakener and not part of an I/O
        // operation. This is specially recognized by the selector.
        //
        // If we haven't been registered with an event loop yet just silently
        // succeed.
        let iocp = self.iocp();
        if let Some(port) = iocp.port() {
            let status = CompletionStatus::new(0, usize::from(iocp.token()),
                                               0 as *mut _);
            try!(port.post(status));
        }
        Ok(())
    }

    pub fn cleanup(&self) {
        // noop
    }

    fn iocp(&self) -> MutexGuard<Registration> {
        self.iocp.lock().unwrap()
    }
}

impl Evented for Awakener {
    fn register(&self, poll: &Poll, token: Token, _events: EventSet,
                opts: PollOpt) -> io::Result<()> {
        try!(self.iocp().associate(poll::selector(poll), token, opts));
        Ok(())
    }

    fn reregister(&self, poll: &Poll, token: Token, events: EventSet,
                  opts: PollOpt) -> io::Result<()> {
        self.register(poll, token, events, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.iocp().checked_deregister(poll::selector(poll))
    }
}
