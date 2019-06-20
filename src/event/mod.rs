//! Readiness event types and utilities.

mod event;
mod evented;

pub use self::event::Event;
pub use self::evented::Evented;

pub use super::poll::{Events, Iter};
