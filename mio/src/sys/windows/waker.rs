use crate::sys::windows::selector::WAKER_OVERLAPPED;
use crate::sys::windows::Selector;
use crate::Token;

use miow::iocp::{CompletionPort, CompletionStatus};
use std::io;
use std::sync::Arc;

#[derive(Debug)]
pub struct Waker {
    token: Token,
    port: Arc<CompletionPort>,
}

impl Waker {
    pub fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        Ok(Waker {
            token,
            port: selector.clone_port(),
        })
    }

    pub fn wake(&self) -> io::Result<()> {
        // Keep NULL as Overlapped value to notify waking.
        let status = CompletionStatus::new(0, self.token.0, WAKER_OVERLAPPED);
        self.port.post(status)
    }
}
