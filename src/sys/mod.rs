//! Module with system specific types.
//!
//! `Event`: a type alias for the system specific event, e.g.
//!          `kevent` or `epoll_event`.
//! `event`: a module with various helper functions for `Event`, see
//!          `crate::event::Event` for the required functions.

#[cfg(unix)]
pub use self::unix::{
    event, Event, Events, Selector, SourceFd, TcpListener, TcpStream, UdpSocket, Waker,
};

#[cfg(unix)]
mod unix;

#[cfg(windows)]
pub use self::windows::{event, Event, Events, Selector, TcpListener, TcpStream, UdpSocket, Waker};

#[cfg(windows)]
pub(crate) use self::windows::afd; // Used in `list_event_details`.

#[cfg(windows)]
mod windows;
