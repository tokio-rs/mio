#[derive(Debug)]
pub struct Event {
    pub flags: u32,
    pub data: u64,
}

use crate::Token;

use super::selector::{EPOLLERR, EPOLLHUP, EPOLLIN, EPOLLOUT, EPOLLPRI, EPOLLRDHUP};

pub fn token(event: &Event) -> Token {
    Token(event.data as usize)
}

pub fn is_readable(event: &Event) -> bool {
    (event.flags & EPOLLIN) != 0 || (event.flags & EPOLLPRI) != 0
}

pub fn is_writable(event: &Event) -> bool {
    (event.flags & EPOLLOUT) != 0
}

pub fn is_error(event: &Event) -> bool {
    (event.flags & EPOLLERR) != 0
}

pub fn is_hup(event: &Event) -> bool {
    (event.flags & EPOLLHUP) != 0 || (event.flags & EPOLLRDHUP) != 0
}

pub fn is_priority(event: &Event) -> bool {
    (event.flags & EPOLLPRI) != 0
}

pub fn is_aio(_: &Event) -> bool {
    // Not supported.
    false
}

pub fn is_lio(_: &Event) -> bool {
    // Not supported.
    false
}
