use crate::sys::windows::{Selector, SelectorInner};
use crate::Token;

use miow::iocp::CompletionStatus;
use std::io;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct Waker {
    inner: Mutex<WakerInner>,
}

#[derive(Debug)]
struct WakerInner {
    token: Token,
    selector: Arc<SelectorInner>,
}

impl Waker {
    pub fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        Ok(Waker::new_priv(selector.clone_inner(), token))
    }

    pub(super) fn new_priv(selector: Arc<SelectorInner>, token: Token) -> Waker {
        Waker {
            inner: Mutex::new(WakerInner { selector, token }),
        }
    }

    pub fn wake(&self) -> io::Result<()> {
        // Each wakeup notification has NULL as its `OVERLAPPED` pointer to
        // indicate that it's from this waker and not part of an I/O operation.
        // This is specially recognized by the selector.
        //
        // If we haven't been registered with an event loop yet just silently
        // succeed.
        let inner = self.inner.lock().unwrap();
        let status = CompletionStatus::new(0, inner.token.0, 0 as *mut _);
        inner.selector.port().post(status)?;
        Ok(())
    }
}
