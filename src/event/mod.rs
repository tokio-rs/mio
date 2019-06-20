//! Readiness event types and utilities.

mod event;
mod evented;
mod events;

pub use self::event::Event;
pub use self::evented::Evented;
pub use self::events::{Events, Iter};
