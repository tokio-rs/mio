//! Module with system specific types.
//!
//! `Event`: a type alias for the system specific event, e.g.
//!          `kevent` or `epoll_event`.
//! `event`: a module with various helper functions for `Event`, see
//!          `crate::event::Event` for the required functions.

#[cfg(unix)]
pub use self::unix::{
    event, Event, EventedFd, Events, Selector, TcpListener, TcpStream, UdpSocket, Waker,
};

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{
    event, Binding, Event, Events, Overlapped, Selector, TcpListener, TcpStream, UdpSocket, Waker,
};

#[cfg(windows)]
mod windows;
