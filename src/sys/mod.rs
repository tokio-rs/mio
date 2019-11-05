//! Module with system specific types.
//!
//! `Event`: a type alias for the system specific event, e.g.
//!          `kevent` or `epoll_event`.
//! `event`: a module with various helper functions for `Event`, see
//!          `crate::event::Event` for the required functions.

#[cfg(all(unix, feature = "os-poll"))]
pub use self::unix::*;

#[cfg(all(unix, feature = "os-poll"))]
mod unix;

#[cfg(all(windows, feature = "os-poll"))]
pub use self::windows::*;

#[cfg(all(windows, feature = "os-poll"))]
mod windows;

#[cfg(not(feature = "os-poll"))]
mod shell;
#[cfg(not(feature = "os-poll"))]
pub(crate) use self::shell::*;
