use std::sync::{Mutex, MutexGuard};

use {io, Evented, EventSet, PollOpt, Selector, Token};
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
            let status = CompletionStatus::new(0, iocp.token().as_usize(),
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
    fn register(&self, selector: &mut Selector, token: Token, _events: EventSet,
                _opts: PollOpt) -> io::Result<()> {
        self.iocp().associate(selector, token);
        Ok(())
    }

    fn reregister(&self, selector: &mut Selector, token: Token, events: EventSet,
                  opts: PollOpt) -> io::Result<()> {
        self.register(selector, token, events, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.iocp().checked_deregister(selector)
    }
}
