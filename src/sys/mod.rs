//! Module with system specific types.
//!
//! `SysEvent`: must be a type alias for the system specific event, e.g.
//!             `kevent` or `epol_event`.
//! `Event`: **must be** a `transparent` wrapper around `SysEvent`, i.e. the
//!          type must have `#[repr(transparent)]` with only `SysEvent` as
//!          field. This is safety requirement, see `Event::from_sys_event_ref`.
//!          Furthermore on this type a number of methods must be implemented
//!          that are used by `Event` (in the `event` module).

#[cfg(unix)]
pub use self::unix::{
    pipe, set_nonblock, Event, EventedFd, Events, Io, Selector, SysEvent, TcpListener, TcpStream,
    UdpSocket, Waker,
};

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{
    Binding, Event, Events, Overlapped, Selector, SysEvent, TcpListener, TcpStream, UdpSocket,
    Waker,
};

#[cfg(windows)]
mod windows;
