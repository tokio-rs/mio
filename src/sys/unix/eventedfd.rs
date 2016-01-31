use {io, poll, Evented, EventSet, Poll, PollOpt, Token};
use std::os::unix::io::RawFd;

/*
 *
 * ===== EventedFd =====
 *
 */

#[derive(Debug)]

pub struct EventedFd<'a>(pub &'a RawFd);

impl<'a> Evented for EventedFd<'a> {
    fn register(&self, poll: &mut Poll, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        poll::selector_mut(poll).register(*self.0, token, interest, opts)
    }

    fn reregister(&self, poll: &mut Poll, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        poll::selector_mut(poll).reregister(*self.0, token, interest, opts)
    }

    fn deregister(&self, poll: &mut Poll) -> io::Result<()> {
        poll::selector_mut(poll).deregister(*self.0)
    }
}
