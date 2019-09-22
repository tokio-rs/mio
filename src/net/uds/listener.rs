use super::UnixStream;
use crate::event::Source;
use crate::unix::SourceFd;
use crate::{sys, Interests, Registry, Token};

use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::SocketAddr;
use std::path::Path;

/// A non-blocking Unix domain socket server.
#[derive(Debug)]
pub struct UnixListener {
    std: std::os::unix::net::UnixListener,
}

impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified socket.
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        let std = sys::uds::bind_listener(path.as_ref())?;
        Ok(UnixListener { std })
    }

    /// Converts a `std` `UnixListener` to a `mio` `UnixListener`.
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    pub fn from_std(std: std::os::unix::net::UnixListener) -> UnixListener {
        UnixListener { std }
    }

    /// Accepts a new incoming connection to this listener.
    ///
    /// The call is responsible for ensuring that the listening socket is in
    /// non-blocking mode.
    pub fn accept(&self) -> io::Result<Option<(UnixStream, SocketAddr)>> {
        match sys::uds::accept(&self.std)? {
            Some((stream, addr)) => Ok(Some((UnixStream::from_std(stream), addr))),
            None => Ok(None),
        }
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `UnixListener` is a reference to the same socket that this
    /// object references. Both handles can be used to accept incoming
    /// connections and options set on one listener will affect the other.
    pub fn try_clone(&self) -> io::Result<UnixListener> {
        self.std.try_clone().map(|std| UnixListener { std })
    }

    /// Returns the local socket address of this listener.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.std.local_addr()
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.std.take_error()
    }
}

impl Source for UnixListener {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).deregister(registry)
    }
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.std.as_raw_fd()
    }
}

impl IntoRawFd for UnixListener {
    fn into_raw_fd(self) -> RawFd {
        self.std.into_raw_fd()
    }
}

impl FromRawFd for UnixListener {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixListener {
        let std = std::os::unix::net::UnixListener::from_raw_fd(fd);
        UnixListener { std }
    }
}
