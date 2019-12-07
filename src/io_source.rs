#![allow(dead_code)] // Currently unused.

use std::ops::{Deref, DerefMut};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;
use std::{fmt, io};

#[cfg(unix)]
use crate::poll;
#[cfg(debug_assertions)]
use crate::poll::SelectorId;
use crate::sys::IoSourceState;
use crate::{event, Interest, Registry, Token};

/// Adapter for a [`RawFd`] or [`RawSocket`] providing an [`event::Source`]
/// implementation.
///
/// `IoSource` enables registering any FD or socket wrapper with [`Poll`].
///
/// While only implementations for TCP, UDP, and UDS (Unix only) are provided,
/// Mio supports registering any FD or socket that can be registered with the
/// underlying OS selector. `IoSource` provides the necessary bridge.
///
/// [`RawFd`]: std::os::unix::io::RawFd
/// [`RawSocket`]: std::os::windows::io::RawSocket
///
/// # Notes
///
/// To handle the registrations and events properly **all** I/O operations (such
/// as `read`, `write`, etc.) must go through the [`do_io`] method to ensure the
/// internal state is updated accordingly.
///
/// [`Poll`]: crate::Poll
/// [`do_io`]: IoSource::do_io
/*
///
/// # Examples
///
/// Basic usage.
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// use mio::{Interest, Poll, Token};
/// use mio::IoSource;
///
/// use std::net;
///
/// let poll = Poll::new()?;
///
/// // Bind a std TCP listener.
/// let listener = net::TcpListener::bind("127.0.0.1:0")?;
/// // Wrap it in the `IoSource` type.
/// let mut listener = IoSource::new(listener);
///
/// // Register the listener.
/// poll.registry().register(&mut listener, Token(0), Interest::READABLE)?;
/// #     Ok(())
/// # }
/// ```
*/
pub struct IoSource<T> {
    state: IoSourceState,
    inner: T,
    #[cfg(debug_assertions)]
    selector_id: SelectorId,
}

impl<T> IoSource<T> {
    /// Create a new `IoSource`.
    pub fn new(io: T) -> IoSource<T> {
        IoSource {
            state: IoSourceState::new(),
            inner: io,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        }
    }

    /// Execute an I/O operations ensuring that the socket receives more events
    /// if it hit a [`WouldBlock`] error.
    ///
    /// # Notes
    ///
    /// This method is required to be called for **all** I/O operations to
    /// ensure the user will receive events once the socket is ready again after
    /// returning a [`WouldBlock`] error.
    ///
    /// [`WouldBlock`]: io::ErrorKind::WouldBlock
    pub fn do_io<F, R>(&mut self, f: F) -> io::Result<R>
    where
        F: FnOnce(&mut T) -> io::Result<R>,
    {
        self.state.do_io(f, &mut self.inner)
    }

    /// Returns the I/O source, dropping the state.
    ///
    /// # Notes
    ///
    /// To ensure no more events are to be received for this I/O source first
    /// [`deregister`] it.
    ///
    /// [`deregister`]: Registry::deregister
    pub fn into_inner(self) -> T {
        self.inner
    }
}

/// Be careful when using this method. All I/O operations that may block must go
/// through the [`do_io`] method.
///
/// [`do_io`]: IoSource::do_io
impl<T> Deref for IoSource<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Be careful when using this method. All I/O operations that may block must go
/// through the [`do_io`] method.
///
/// [`do_io`]: IoSource::do_io
impl<T> DerefMut for IoSource<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(unix)]
impl<T> event::Source for IoSource<T>
where
    T: AsRawFd,
{
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        #[cfg(debug_assertions)]
        self.selector_id.associate_selector(registry)?;
        poll::selector(registry).register(self.inner.as_raw_fd(), token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        poll::selector(registry).reregister(self.inner.as_raw_fd(), token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        poll::selector(registry).deregister(self.inner.as_raw_fd())
    }
}

#[cfg(windows)]
impl<T> event::Source for IoSource<T>
where
    T: AsRawSocket,
{
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        #[cfg(debug_assertions)]
        self.selector_id.associate_selector(registry)?;
        self.state
            .register(registry, token, interests, self.inner.as_raw_socket())
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.state.reregister(registry, token, interests)
    }

    fn deregister(&mut self, _registry: &Registry) -> io::Result<()> {
        self.state.deregister()
    }
}

impl<T> fmt::Debug for IoSource<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}
