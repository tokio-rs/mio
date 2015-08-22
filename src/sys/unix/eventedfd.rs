use {io, Evented, EventSet, PollOpt, Selector, Token};
use std::os::unix::io::RawFd;

/*
 *
 * ===== EventedFd =====
 *
 */

#[derive(Debug)]

pub struct EventedFd<'a>(pub &'a RawFd);

impl<'a> Evented for EventedFd<'a> {
    fn register(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        selector.register(*self.0, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        selector.reregister(*self.0, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        selector.deregister(*self.0)
    }
}
