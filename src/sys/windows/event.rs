use miow::iocp::CompletionStatus;

use super::afd;
use crate::Token;

use std::fmt;

#[derive(Debug)]
pub struct Event {
    pub flags: u32,
    pub data: u64,
}

pub fn token(event: &Event) -> Token {
    Token(event.data as usize)
}

pub fn is_readable(event: &Event) -> bool {
    event.flags
        & (afd::POLL_RECEIVE | afd::POLL_DISCONNECT | afd::POLL_ACCEPT | afd::POLL_CONNECT_FAIL)
        != 0
}

pub fn is_writable(event: &Event) -> bool {
    event.flags & (afd::POLL_SEND | afd::POLL_CONNECT_FAIL) != 0
}

pub fn is_error(event: &Event) -> bool {
    event.flags & afd::POLL_CONNECT_FAIL != 0
}

pub fn is_read_closed(event: &Event) -> bool {
    event.flags & afd::POLL_DISCONNECT != 0
}

pub fn is_write_closed(event: &Event) -> bool {
    event.flags & (afd::POLL_ABORT | afd::POLL_CONNECT_FAIL) != 0
}

pub fn is_priority(event: &Event) -> bool {
    event.flags & afd::POLL_RECEIVE_EXPEDITED != 0
}

pub fn is_aio(_: &Event) -> bool {
    // Not supported.
    false
}

pub fn is_lio(_: &Event) -> bool {
    // Not supported.
    false
}

pub fn write_details(f: &mut fmt::Formatter<'_>, event: &Event) {
    macro_rules! has_event {
            ($($(#[$target: meta])* $event: ident),+ $(,)*) => {
                $(
                    $(#[$target])*
                    {
                        if event.flags & afd::$event != 0 {
                            write!(f, "{} ", stringify!($event)).unwrap();
                        }
                    }
                )+
            };
        }

    has_event!(
        POLL_RECEIVE,
        POLL_RECEIVE_EXPEDITED,
        POLL_SEND,
        POLL_DISCONNECT,
        POLL_ABORT,
        POLL_LOCAL_CLOSE,
        POLL_ACCEPT,
        POLL_CONNECT_FAIL,
    );

    ()
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
        self.events.clear();
        for status in self.statuses.iter_mut() {
            *status = CompletionStatus::zero();
        }
    }
}
