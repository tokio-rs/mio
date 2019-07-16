#[derive(Debug)]
pub struct Event {
    pub flags: u32,
    pub data: u64,
}

use super::afd;
use crate::Token;

pub fn token(event: &Event) -> Token {
    Token(event.data as usize)
}

pub fn is_readable(event: &Event) -> bool {
    (event.flags & (afd::KNOWN_AFD_EVENTS & !afd::AFD_POLL_SEND)) != 0
}

pub fn is_writable(event: &Event) -> bool {
    (event.flags & (afd::AFD_POLL_SEND | afd::AFD_POLL_CONNECT_FAIL)) != 0
}

pub fn is_error(event: &Event) -> bool {
    event.flags == afd::AFD_POLL_CONNECT_FAIL
}

pub fn is_hup(event: &Event) -> bool {
    (event.flags & (afd::AFD_POLL_ABORT | afd::AFD_POLL_CONNECT_FAIL)) != 0
}

pub fn is_priority(event: &Event) -> bool {
    (event.flags & afd::AFD_POLL_RECEIVE_EXPEDITED) != 0
}

pub fn is_aio(_: &Event) -> bool {
    // Not supported.
    false
}

pub fn is_lio(_: &Event) -> bool {
    // Not supported.
    false
}

pub type Events = Vec<Event>;
