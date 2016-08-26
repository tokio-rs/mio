use {io, poll, Evented, Ready, Poll, PollOpt, Token};
use std::os::unix::io::RawFd;

/*
 *
 * ===== EventedFd =====
 *
 */

#[derive(Debug)]

pub struct EventedFd<'a>(pub &'a RawFd);

impl<'a> Evented for EventedFd<'a> {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        poll::selector(poll).register(*self.0, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        poll::selector(poll).reregister(*self.0, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        poll::selector(poll).deregister(*self.0)
    }
}
