use crate::io_source::IoSource;
use crate::net::{SocketAddr, UnixStream};
use crate::{event, sys, Interest, Registry, Token};

#[cfg(windows)]
use crate::sys::windows::stdnet as net;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
#[cfg(unix)]
use std::os::unix::net;
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::path::Path;
use std::{fmt, io};

/// A non-blocking Unix domain socket server.
pub struct UnixListener {
    inner: IoSource<net::UnixListener>,
}

impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified socket.
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        sys::uds::listener::bind(path.as_ref()).map(UnixListener::from_std)
    }

    /// Creates a new `UnixListener` from a standard `net::UnixListener`.
    ///
    /// This function is intended to be used to wrap a Unix listener from the
    /// standard library in the Mio equivalent. The conversion assumes nothing
    /// about the underlying listener; it is left up to the user to set it in
    /// non-blocking mode.
    #[cfg(unix)]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    pub fn from_std(listener: net::UnixListener) -> UnixListener {
        UnixListener {
            inner: IoSource::new(listener),
        }
    }

    #[cfg(windows)]
    pub(crate) fn from_std(listener: net::UnixListener) -> UnixListener {
        UnixListener {
            inner: IoSource::new(listener),
        }
    }

    /// Accepts a new incoming connection to this listener.
    ///
    /// The call is responsible for ensuring that the listening socket is in
    /// non-blocking mode.
    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        self.inner.do_io(sys::uds::listener::accept)
            .map(|(stream, addr)| (stream, SocketAddr::new(addr)))
    }

    /// Returns the local socket address of this listener.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        sys::uds::listener::local_addr(&self.inner).map(|addr| SocketAddr::new(addr))
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

#[cfg(unix)]
#[cfg_attr(docsrs, doc(cfg(unix)))]
impl IntoRawFd for UnixListener {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_inner().into_raw_fd()
    }
}

#[cfg(unix)]
#[cfg_attr(docsrs, doc(cfg(unix)))]
impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

#[cfg(unix)]
#[cfg_attr(docsrs, doc(cfg(unix)))]
impl FromRawFd for UnixListener {
    /// Converts a `RawFd` to a `UnixListener`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> UnixListener {
        UnixListener::from_std(FromRawFd::from_raw_fd(fd))
    }
}

#[cfg(windows)]
#[cfg_attr(docsrs, doc(cfg(windows)))]
impl IntoRawSocket for UnixListener {
    fn into_raw_socket(self) -> RawSocket {
        self.inner.into_inner().into_raw_socket()
    }
}

#[cfg(windows)]
#[cfg_attr(docsrs, doc(cfg(windows)))]
impl AsRawSocket for UnixListener {
    fn as_raw_socket(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

#[cfg(windows)]
#[cfg_attr(docsrs, doc(cfg(windows)))]
impl FromRawSocket for UnixListener {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        UnixListener::from_std(FromRawSocket::from_raw_socket(sock))
    }
}
