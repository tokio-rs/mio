//! Module with system specific types.
//!
//! Required types:
//!
//! * `Event`: a type alias for the system specific event, e.g. `kevent` or
//!            `epoll_event`.
//! * `event`: a module with various helper functions for `Event`, see
//!            [`crate::event::Event`] for the required functions.
//! * `Events`: collection of `Event`s, see [`crate::Events`].
//! * `Selector`: selector used to register event sources and poll for events,
//!               see [`crate::Poll`] and [`crate::Registry`] for required
//!               methods.
//! * `TcpListener`, `TcpStream` and `UdpSocket`: see [`crate::net`] module.
//! * `Waker`: see [`crate::Waker`].

#[cfg(unix)]
pub use self::unix::{
    event, Event, Events, Selector, SocketAddr, SourceFd, TcpListener, TcpStream, UdpSocket,
    UnixDatagram, UnixListener, UnixStream, Waker,
};

#[cfg(unix)]
mod unix;

#[cfg(windows)]
pub use self::windows::{event, Event, Events, Selector, TcpListener, TcpStream, UdpSocket, Waker};

#[cfg(windows)]
mod windows;
