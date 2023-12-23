//! Readiness event types and utilities.

#[allow(clippy::module_inception)]
mod event;
mod events;
mod source;

pub use event::Event;
pub use events::{Events, Iter};
pub use source::Source;
