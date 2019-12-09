use crate::net::{SocketAddr, UnixStream};
#[cfg(debug_assertions)]
use crate::poll::SelectorId;
use crate::{event, sys, Interest, Registry, Token};

use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net;
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

    /// Creates a new `UnixListener` from a standard `net::UnixListener`.
    ///
    /// This function is intended to be used to wrap a Unix listener from the
    /// standard library in the Mio equivalent. The conversion assumes nothing
    /// about the underlying listener; it is left up to the user to set it in
    /// non-blocking mode.
    pub fn from_std(listener: net::UnixListener) -> UnixListener {
        let sys = sys::UnixListener::from_std(listener);
        UnixListener {
            sys,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        }
    }

    /// Accepts a new incoming connection to this listener.
    ///
    /// The call is responsible for ensuring that the listening socket is in
    /// non-blocking mode.
    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        let (sys, sockaddr) = self.sys.accept()?;
        Ok((UnixStream::new(sys), sockaddr))
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

impl event::Source for UnixListener {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        #[cfg(debug_assertions)]
        self.selector_id.associate_selector(registry)?;
        self.sys.register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.sys.reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
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
