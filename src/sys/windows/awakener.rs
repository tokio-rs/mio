use crate::sys::windows::{Selector, SelectorInner};
use crate::{io, Token};
use miow::iocp::CompletionStatus;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct Awakener {
    inner: Mutex<AwakenerInner>,
}

#[derive(Debug)]
struct AwakenerInner {
    token: Token,
    selector: Arc<SelectorInner>,
}

impl Awakener {
    pub fn new(selector: &Selector, token: Token) -> io::Result<Awakener> {
        Ok(Awakener::new_priv(selector.clone_inner(), token))
    }

    pub(super) fn new_priv(selector: Arc<SelectorInner>, token: Token) -> Awakener {
        Awakener {
            inner: Mutex::new(AwakenerInner { selector, token }),
        }
    }

    pub fn wake(&self) -> io::Result<()> {
        // Each wakeup notification has NULL as its `OVERLAPPED` pointer to
        // indicate that it's from this awakener and not part of an I/O
        // operation. This is specially recognized by the selector.
        //
        // If we haven't been registered with an event loop yet just silently
        // succeed.
        let inner = self.inner.lock().unwrap();
        let status = CompletionStatus::new(0, inner.token.0, 0 as *mut _);
        inner.selector.port().post(status)?;
        Ok(())
    }
}
