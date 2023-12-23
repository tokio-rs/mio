use std::io;
use std::sync::Arc;

use crate::sys::windows::iocp::CompletionPort;
use crate::sys::windows::{Event, Selector};
use crate::Token;

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
        let mut ev = Event::new(self.token);
        ev.set_readable();

        self.port.post(ev.to_completion_status())
    }
}
