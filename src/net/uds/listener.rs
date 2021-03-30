use crate::io_source::IoSource;
use crate::net::{SocketAddr, UnixStream};
use crate::{event, sys, Interest, Registry, Token};

use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::path::Path;
use std::{fmt, io};

/// A non-blocking Unix domain socket server.
pub struct UnixListener {
    inner: IoSource<sys::net::UnixListener>,
}

impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified socket.
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        sys::uds::listener::bind(path.as_ref()).map(Self::from_std)
    }

    /// Creates a new `UnixListener` from a standard `net::UnixListener`.
    ///
    /// This function is intended to be used to wrap a Unix listener from the
    /// standard library in the Mio equivalent. The conversion assumes nothing
    /// about the underlying listener; it is left up to the user to set it in
    /// non-blocking mode.
    pub fn from_std(listener: sys::net::UnixListener) -> Self {
        Self {
            inner: IoSource::new(listener),
        }
    }

    /// Accepts a new incoming connection to this listener.
    ///
    /// The call is responsible for ensuring that the listening socket is in
    /// non-blocking mode.
    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        sys::uds::listener::accept(&self.inner)
    }

    /// Returns the local socket address of this listener.
    pub fn local_addr(&self) -> io::Result<sys::uds::SocketAddr> {
        sys::uds::listener::local_addr(&self.inner)
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }
}

impl event::Source for UnixListener {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.inner.register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.inner.reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        self.inner.deregister(registry)
    }
}

impl fmt::Debug for UnixListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl IntoRawFd for UnixListener {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_inner().into_raw_fd()
    }
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl FromRawFd for UnixListener {
    /// Converts a `RawFd` to a `UnixListener`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self::from_std(FromRawFd::from_raw_fd(fd))
    }
}
