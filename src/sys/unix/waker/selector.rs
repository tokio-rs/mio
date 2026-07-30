use std::io;

use crate::sys::Selector;
use crate::Token;

/// Waker implementation that calls `wake` on the `Selector`.
#[derive(Debug)]
pub(crate) struct Waker {
    selector: Selector,
    token: Token,
}

impl Waker {
    pub(crate) fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        let selector = selector.try_clone()?;
        Ok(Waker { selector, token })
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        self.selector.wake(self.token)
    }
}
