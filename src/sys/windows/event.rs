use crate::Token;

use super::Ready;

#[derive(Debug, Clone)]
pub struct Event {
    token: Token,
    readiness: Ready,
}

impl Event {
    pub(crate) fn new(readiness: Ready, token: Token) -> Event {
        Event { token, readiness }
    }
}

pub fn token(event: &Event) -> Token {
    event.token
}

pub fn is_readable(event: &Event) -> bool {
    event.readiness.is_readable()
}

pub fn is_writable(event: &Event) -> bool {
    event.readiness.is_writable()
}

pub fn is_error(event: &Event) -> bool {
    event.readiness.is_error()
}

pub fn is_hup(event: &Event) -> bool {
    event.readiness.is_hup()
}

pub fn is_priority(event: &Event) -> bool {
    event.readiness.is_priority()
}

pub fn is_aio(event: &Event) -> bool {
    event.readiness.is_aio()
}

pub fn is_lio(event: &Event) -> bool {
    event.readiness.is_lio()
}
