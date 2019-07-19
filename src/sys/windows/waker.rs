use crate::sys::windows::{Selector, SelectorInner};
use crate::Token;

use miow::iocp::CompletionStatus;
use std::io;
use std::sync::Arc;

#[derive(Debug)]
pub struct Waker {
    token: Token,
    selector: Arc<SelectorInner>,
}

impl Waker {
    pub fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        Ok(Waker {
            token,
            selector: selector.clone_inner(),
        })
    }

    pub fn wake(&self) -> io::Result<()> {
        // Keep NULL as Overlapped value to notify waking.
        let status = CompletionStatus::new(0, self.token.0, 0 as *mut _);
        self.selector.port().post(status)
    }
}
