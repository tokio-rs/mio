use crate::Token;

use super::Ready;

pub type SysEvent = Event;

#[derive(Debug, Clone)]
pub struct Event {
    token: Token,
    readiness: Ready,
}

impl Event {
    pub(crate) fn new(readiness: Ready, token: Token) -> Event {
        Event { token, readiness }
    }

    pub fn token(&self) -> Token {
        self.token
    }

    pub fn is_readable(&self) -> bool {
        self.readiness.is_readable()
    }

    pub fn is_writable(&self) -> bool {
        self.readiness.is_writable()
    }

    pub fn is_error(&self) -> bool {
        self.readiness.is_error()
    }

    pub fn is_hup(&self) -> bool {
        self.readiness.is_hup()
    }

    pub fn is_priority(&self) -> bool {
        self.readiness.is_priority()
    }

    pub fn is_aio(&self) -> bool {
        self.readiness.is_aio()
    }

    pub fn is_lio(&self) -> bool {
        self.readiness.is_lio()
    }
}
