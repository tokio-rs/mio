use crate::event::Source;
#[cfg(debug_assertions)]
use crate::poll::SelectorId;
use crate::unix::SourceFd;
use crate::{sys, Interests, Registry, Token};

use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::SocketAddr;
use std::path::Path;

/// A Unix datagram socket.
#[derive(Debug)]
pub struct UnixDatagram {
    std: std::os::unix::net::UnixDatagram,
    #[cfg(debug_assertions)]
    selector_id: SelectorId,
}

impl UnixDatagram {
    /// Creates a Unix datagram socket bound to the given path.
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixDatagram> {
        let std = sys::uds::bind_datagram(path.as_ref())?;
        Ok(UnixDatagram {
            std,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        })
    }

    /// Converts a `std` `UnixDatagram` to a `mio` `UnixDatagram`.
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    pub fn from_std(std: std::os::unix::net::UnixDatagram) -> UnixDatagram {
        UnixDatagram {
            std,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        }
    }

    /// Creates a Unix Datagram socket which is not bound to any address.
    pub fn unbound() -> io::Result<UnixDatagram> {
        let std = sys::uds::unbound_datagram()?;
        Ok(UnixDatagram {
            std,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        })
    }

    /// Create an unnamed pair of connected sockets.
    pub fn pair() -> io::Result<(UnixDatagram, UnixDatagram)> {
        let (a, b) = sys::uds::pair_datagram()?;
        Ok((UnixDatagram::from_std(a), UnixDatagram::from_std(b)))
    }

    /// Connects the socket to the specified address.
    pub fn connect<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self.std.connect(path)
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `UnixListener` is a reference to the same socket that this
    /// object references. Both handles can be used to accept incoming
    /// connections and options set on one listener will affect the other.
    pub fn try_clone(&self) -> io::Result<UnixDatagram> {
        self.std.try_clone().map(Self::from_std)
    }

    /// Returns the address of this socket.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.std.local_addr()
    }

    /// Returns the address of this socket's peer.
    ///
    /// The `connect` method will connect the socket to a peer.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.std.peer_addr()
    }

    /// Receives data from the socket.
    ///
    /// On success, returns the number of bytes read and the address from
    /// whence the data came.
    pub fn recv_from(&self, dst: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.std.recv_from(dst)
    }

    /// Receives data from the socket.
    ///
    /// On success, returns the number of bytes read.
    pub fn recv(&self, dst: &mut [u8]) -> io::Result<usize> {
        self.std.recv(dst)
    }

    /// Sends data on the socket to the specified address.
    ///
    /// On success, returns the number of bytes written.
    pub fn send_to<P: AsRef<Path>>(&self, src: &[u8], path: P) -> io::Result<usize> {
        self.std.send_to(src, path)
    }

    /// Sends data on the socket to the socket's peer.
    ///
    /// The peer address may be set by the `connect` method, and this method
    /// will return an error if the socket has not already been connected.
    ///
    /// On success, returns the number of bytes written.
    pub fn send(&self, src: &[u8]) -> io::Result<usize> {
        self.std.send(src)
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.std.take_error()
    }

    /// Shut down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.std.shutdown(how)
    }
}

impl Source for UnixDatagram {
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

impl AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> RawFd {
        self.std.as_raw_fd()
    }
}

impl IntoRawFd for UnixDatagram {
    fn into_raw_fd(self) -> RawFd {
        self.std.into_raw_fd()
    }
}

impl FromRawFd for UnixDatagram {
    /// Converts a `std` `RawFd` to a `mio` `UnixDatagram`.
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> UnixDatagram {
        let std = std::os::unix::net::UnixDatagram::from_raw_fd(fd);
        UnixDatagram {
            std,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        }
    }
}
