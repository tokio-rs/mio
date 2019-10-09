use crate::event::Source;
use crate::net::UnixStream;
#[cfg(debug_assertions)]
use crate::poll::SelectorId;
use crate::unix::SocketAddr;
use crate::{sys, Interests, Registry, Token};

use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::path::Path;

/// A non-blocking Unix domain socket server.
#[derive(Debug)]
pub struct UnixListener {
    sys: sys::UnixListener,
    #[cfg(debug_assertions)]
    selector_id: SelectorId,
}

impl UnixListener {
    fn new(sys: sys::UnixListener) -> UnixListener {
        UnixListener {
            sys,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        }
    }

    /// Creates a new `UnixListener` bound to the specified socket.
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        let sys = sys::UnixListener::bind(path.as_ref())?;
        Ok(UnixListener::new(sys))
    }

    /// Accepts a new incoming connection to this listener.
    ///
    /// The call is responsible for ensuring that the listening socket is in
    /// non-blocking mode.
    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        let (sys, sockaddr) = self.sys.accept()?;
        Ok((UnixStream::new(sys), sockaddr))
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `UnixListener` is a reference to the same socket that this
    /// object references. Both handles can be used to accept incoming
    /// connections and options set on one listener will affect the other.
    pub fn try_clone(&self) -> io::Result<UnixListener> {
        let sys = self.sys.try_clone()?;
        Ok(UnixListener::new(sys))
    }

    /// Returns the local socket address of this listener.
    pub fn local_addr(&self) -> io::Result<sys::SocketAddr> {
        self.sys.local_addr()
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.sys.take_error()
    }
}

impl Source for UnixListener {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        #[cfg(debug_assertions)]
        self.selector_id.associate_selector(registry)?;
        self.sys.reregister(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.sys.reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.sys.deregister(registry)
    }
}

#[cfg(unix)]
impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

#[cfg(unix)]
impl IntoRawFd for UnixListener {
    fn into_raw_fd(self) -> RawFd {
        self.sys.into_raw_fd()
    }
}

#[cfg(unix)]
impl FromRawFd for UnixListener {
    /// Converts a `std` `RawFd` to a `mio` `UnixListener`.
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> UnixListener {
        UnixListener::new(FromRawFd::from_raw_fd(fd))
    }
}
