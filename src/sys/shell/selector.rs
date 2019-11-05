use std::io;
use std::time::Duration;

#[derive(Debug)]
pub struct Selector {
}

pub type Event = usize;

pub type Events = Vec<Event>;

impl Selector {
    pub fn new() -> io::Result<Selector> {
        os_required!();
    }

    #[cfg(debug_assertions)]
    pub fn id(&self) -> usize {
        os_required!();
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        os_required!();
    }

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        os_required!();
    }

    /*
    pub fn register(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        os_required!();
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        os_required!();
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        os_required!();
    }
    */
}

pub mod event {
    use crate::sys::Event;
    use crate::Token;

    pub fn token(event: &Event) -> Token {
        os_required!();
    }

    pub fn is_readable(event: &Event) -> bool {
        os_required!();
    }

    pub fn is_writable(event: &Event) -> bool {
        os_required!();
    }

    pub fn is_error(event: &Event) -> bool {
        os_required!();
    }

    pub fn is_read_closed(event: &Event) -> bool {
        os_required!();
    }

    pub fn is_write_closed(event: &Event) -> bool {
        os_required!();
    }

    pub fn is_priority(event: &Event) -> bool {
        os_required!();
    }

    pub fn is_aio(_: &Event) -> bool {
        os_required!();
    }

    pub fn is_lio(_: &Event) -> bool {
        os_required!();
    }
}
