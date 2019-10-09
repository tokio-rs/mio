use crate::event::Source;
#[cfg(debug_assertions)]
use crate::poll::SelectorId;
use crate::{sys, Interests, Registry, Token};

use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::path::Path;

/// A Unix datagram socket.
#[derive(Debug)]
pub struct UnixDatagram {
    sys: sys::UnixDatagram,
    #[cfg(debug_assertions)]
    selector_id: SelectorId,
}

impl UnixDatagram {
    fn new(sys: sys::UnixDatagram) -> UnixDatagram {
        UnixDatagram {
            sys,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        }
    }

    /// Creates a Unix datagram socket bound to the given path.
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixDatagram> {
        let sys = sys::UnixDatagram::bind(path.as_ref())?;
        Ok(UnixDatagram::new(sys))
    }

    /// Connects the socket to the specified address.
    pub fn connect<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self.sys.connect(path)
    }

    /// Creates a Unix Datagram socket which is not bound to any address.
    pub fn unbound() -> io::Result<UnixDatagram> {
        let sys = sys::UnixDatagram::unbound()?;
        Ok(UnixDatagram::new(sys))
    }

    /// Create an unnamed pair of connected sockets.
    pub fn pair() -> io::Result<(UnixDatagram, UnixDatagram)> {
        let (a, b) = sys::UnixDatagram::pair()?;
        let a = UnixDatagram::new(a);
        let b = UnixDatagram::new(b);
        Ok((a, b))
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `UnixListener` is a reference to the same socket that this
    /// object references. Both handles can be used to accept incoming
    /// connections and options set on one listener will affect the other.
    pub fn try_clone(&self) -> io::Result<UnixDatagram> {
        let sys = self.sys.try_clone()?;
        Ok(UnixDatagram::new(sys))
    }

    /// Returns the address of this socket.
    pub fn local_addr(&self) -> io::Result<sys::SocketAddr> {
        self.sys.local_addr()
    }

    /// Returns the address of this socket's peer.
    ///
    /// The `connect` method will connect the socket to a peer.
    pub fn peer_addr(&self) -> io::Result<sys::SocketAddr> {
        self.sys.peer_addr()
    }

    /// Receives data from the socket.
    ///
    /// On success, returns the number of bytes read and the address from
    /// whence the data came.
    pub fn recv_from(&self, dst: &mut [u8]) -> io::Result<(usize, sys::SocketAddr)> {
        self.sys.recv_from(dst)
    }

    /// Receives data from the socket.
    ///
    /// On success, returns the number of bytes read.
    pub fn recv(&self, dst: &mut [u8]) -> io::Result<usize> {
        self.sys.recv(dst)
    }

    /// Sends data on the socket to the specified address.
    ///
    /// On success, returns the number of bytes written.
    pub fn send_to<P: AsRef<Path>>(&self, src: &[u8], path: P) -> io::Result<usize> {
        self.sys.send_to(src, path)
    }

    /// Sends data on the socket to the socket's peer.
    ///
    /// The peer address may be set by the `connect` method, and this method
    /// will return an error if the socket has not already been connected.
    ///
    /// On success, returns the number of bytes written.
    pub fn send(&self, src: &[u8]) -> io::Result<usize> {
        self.sys.send(src)
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.sys.take_error()
    }

    /// Shut down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.sys.shutdown(how)
    }
}

impl Source for UnixDatagram {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        #[cfg(debug_assertions)]
        self.selector_id.associate_selector(registry)?;
        self.sys.register(registry, token, interests)
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

impl AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

impl FromRawFd for UnixDatagram {
    /// Converts a `std` `RawFd` to a `mio` `UnixDatagram`.
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> UnixDatagram {
        UnixDatagram::new(FromRawFd::from_raw_fd(fd))
    }
}

impl IntoRawFd for UnixDatagram {
    fn into_raw_fd(self) -> RawFd {
        self.sys.into_raw_fd()
    }
}
