use std::sync::{Mutex, Arc};

use {io, Evented, EventSet, PollOpt, Selector, Token};
use sys::windows::selector::SelectorInner;
use wio::iocp::CompletionStatus;

pub struct Awakener {
    iocp: Mutex<Option<Registration>>,
}

struct Registration {
    iocp: Arc<SelectorInner>,
    token: Token,
}

impl Awakener {
    pub fn new() -> io::Result<Awakener> {
        Ok(Awakener {
            iocp: Mutex::new(None),
        })
    }

    pub fn wakeup(&self) -> io::Result<()> {
        // Each wakeup notification has NULL as its `OVERLAPPED` pointer to
        // indicate that it's from this awakener and not part of an I/O
        // operation. This is specially recognized by the selector.
        //
        // If we haven't been registered with an event loop yet just silently
        // succeed.
        let iocp = self.iocp.lock().unwrap();
        if let Some(ref r) = *iocp {
            let status = CompletionStatus::new(0, r.token.as_usize(),
                                               0 as *mut _);
            try!(r.iocp.port().post(status));
        }
        Ok(())
    }

    pub fn cleanup(&self) {
        // noop
    }
}

impl Evented for Awakener {
    fn register(&self, selector: &mut Selector, token: Token, _events: EventSet,
                _opts: PollOpt) -> io::Result<()> {
        *self.iocp.lock().unwrap() = Some(Registration {
            iocp: selector.inner().clone(),
            token: token,
        });
        Ok(())
    }

    fn reregister(&self, selector: &mut Selector, token: Token, events: EventSet,
                  opts: PollOpt) -> io::Result<()> {
        self.register(selector, token, events, opts)
    }

    fn deregister(&self, _selector: &mut Selector) -> io::Result<()> {
        *self.iocp.lock().unwrap() = None;
        Ok(())
    }
}
