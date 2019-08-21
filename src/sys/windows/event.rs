#[derive(Debug)]
pub struct Event {
    pub flags: u32,
    pub data: u64,
}

use miow::iocp::CompletionStatus;

use super::afd;
use crate::Token;

pub fn token(event: &Event) -> Token {
    Token(event.data as usize)
}

pub fn is_readable(event: &Event) -> bool {
    if is_error(event) || is_read_hup(event) {
        return true;
    }
    event.flags & (afd::AFD_POLL_RECEIVE | afd::AFD_POLL_ACCEPT) != 0
}

pub fn is_writable(event: &Event) -> bool {
    if is_error(event) {
        return true;
    }
    event.flags & afd::AFD_POLL_SEND != 0
}

pub fn is_error(event: &Event) -> bool {
    event.flags & afd::AFD_POLL_CONNECT_FAIL != 0
}

pub fn is_hup(event: &Event) -> bool {
    event.flags & afd::AFD_POLL_ABORT != 0
}

pub fn is_read_hup(event: &Event) -> bool {
    event.flags & afd::AFD_POLL_DISCONNECT != 0
}

pub fn is_priority(event: &Event) -> bool {
    event.flags & afd::AFD_POLL_RECEIVE_EXPEDITED != 0
}

pub fn is_aio(_: &Event) -> bool {
    // Not supported.
    false
}

pub fn is_lio(_: &Event) -> bool {
    // Not supported.
    false
}

pub struct Events {
    /// Raw I/O event completions are filled in here by the call to `get_many`
    /// on the completion port above. These are then processed to run callbacks
    /// which figure out what to do after the event is done.
    pub statuses: Box<[CompletionStatus]>,

    /// Literal events returned by `get` to the upwards `EventLoop`. This file
    /// doesn't really modify this (except for the waker), instead almost all
    /// events are filled in by the `ReadinessQueue` from the `poll` module.
    pub events: Vec<Event>,
}

impl Events {
    pub fn with_capacity(cap: usize) -> Events {
        // Note that it's possible for the output `events` to grow beyond the
        // capacity as it can also include deferred events, but that's certainly
        // not the end of the world!
        Events {
            statuses: vec![CompletionStatus::zero(); cap].into_boxed_slice(),
            events: Vec::with_capacity(cap),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.events.capacity()
    }

    pub fn get(&self, idx: usize) -> Option<&Event> {
        self.events.get(idx)
    }

    pub fn clear(&mut self) {
        self.events.truncate(0);
        for c in 0..self.statuses.len() {
            self.statuses[c] = CompletionStatus::zero();
        }
    }
}
