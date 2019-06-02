use std::io;
use std::time::Duration;

use crate::{Token, Interests, PollOpt};

/// The backend trait for system specific selectors.
///
/// This is used by [`Poll`] to delegate to the platform specific selector.
pub trait Backend: Sized {
    /// The type used for [`Evented`] handles.
    ///
    /// On Unix like systems this will be file descriptors, on Windows this
    /// would be a handle.
    type RawHandle;

    /// Create a new `Selector` backend.
    fn new() -> io::Result<Self>;

    /// Poll for new events.
    fn poll(
        &mut self,
        events: &mut Events,
        awakener: Token,
        timeout: Option<Duration>,
    ) -> io::Result<bool>;

    /// Register a new [`Evented`] handle.
    fn register(
        &mut self,
        handle: Self::RawHandle,
        token: Token,
        interests: Interests,
        options: PollOpt,
    ) -> io::Result<()>;

    /// Re-register an already registered [`Evented`] handle.
    fn reregister(
        &mut self,
        handle: Self::RawHandle,
        token: Token,
        interests: Interests,
        options: PollOpt,
    ) -> io::Result<()>;

    /// Deregister a [`Evented`] handle.
    fn deregister(&mut self, handle: Self::RawHandle) -> io::Result<()>;
}

#[cfg(unix)]
pub use self::unix::{
    pipe, set_nonblock, Awakener, EventedFd, Events, Io, Selector, TcpListener, TcpStream,
    UdpSocket,
};

#[cfg(unix)]
pub use self::unix::READY_ALL;

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{
    Awakener, Binding, Events, Overlapped, Selector, TcpListener, TcpStream, UdpSocket,
};

#[cfg(windows)]
mod windows;
