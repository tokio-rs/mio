use crate::net::{TcpStream, TcpListener};
use crate::sys;

use std::io;
use std::mem;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd, FromRawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};

/// A non-blocking TCP socket used to configure a stream or listener.
///
/// The `TcpSocket` type wraps the operating-system's socket handle. This type
/// is used to configure the socket before establishing a connection or start
/// listening for inbound connections.
///
/// The socket will be closed when the value is dropped.
#[derive(Debug)]
pub struct TcpSocket {
    sys: sys::tcp::TcpSocket,
}

impl TcpSocket {
    /// Create a new IPv4 TCP socket.
    ///
    /// This calls `socket(2)`.
    pub fn new_v4() -> io::Result<TcpSocket> {
        sys::tcp::new_v4_socket().map(|sys| TcpSocket {
            sys
        })
    }

    /// Create a new IPv6 TCP socket.
    ///
    /// This calls `socket(2)`.
    pub fn new_v6() -> io::Result<TcpSocket> {
        sys::tcp::new_v6_socket().map(|sys| TcpSocket {
            sys
        })
    }

    pub(crate) fn new_for_addr(addr: SocketAddr) -> io::Result<TcpSocket> {
        if addr.is_ipv4() {
            TcpSocket::new_v4()
        } else {
            TcpSocket::new_v6()
        }
    }

    /// Bind `addr` to the TCP socket.
    pub fn bind(&self, addr: SocketAddr) -> io::Result<()> {
        sys::tcp::bind(self.sys, addr)
    }

    /// Connect the socket to `addr`.
    ///
    /// This consumes the socket and performs the connect operation. Once the
    /// connection completes, the socket is now a non-blocking `TcpStream` and
    /// can be used as such.
    pub fn connect(self, addr: SocketAddr) -> io::Result<TcpStream> {
        let stream = sys::tcp::connect(self.sys, addr)?;

        // Don't close the socket
        mem::forget(self);
        Ok(TcpStream::from_std(stream))
    }

    /// Listen for inbound connections, converting the socket to a
    /// `TcpListener`.
    pub fn listen(self, backlog: u32) -> io::Result<TcpListener> {
        let listener = sys::tcp::listen(self.sys, backlog)?;

        // Don't close the socket
        mem::forget(self);
        Ok(TcpListener::from_std(listener))
    }

    /// Sets the value of `SO_REUSEADDR` on this socket.
    pub fn set_reuseaddr(&self, reuseaddr: bool) -> io::Result<()> {
        sys::tcp::set_reuseaddr(self.sys, reuseaddr)
    }
}

impl Drop for TcpSocket {
    fn drop(&mut self) {
        sys::tcp::close(self.sys);
    }
}

#[cfg(unix)]
impl AsRawFd for TcpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.sys
    }
}

#[cfg(unix)]
impl FromRawFd for TcpSocket {
    /// Converts a `RawFd` to a `TcpSocket`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> TcpSocket {
        TcpSocket { sys: fd }
    }
}

#[cfg(windows)]
impl IntoRawSocket for TcpSocket {
    fn into_raw_socket(self) -> RawSocket {
        // The winapi crate defines `SOCKET` as `usize`. The Rust std
        // conditionally defines `RawSocket` as a fixed size unsigned integer
        // matching the pointer width. These end up being the same type but we
        // must cast between them.
        let ret = self.sys as RawSocket;

        // Avoid closing the socket
        mem::forget(self);

        ret
    }
}

#[cfg(windows)]
impl AsRawSocket for TcpSocket {
    fn as_raw_socket(&self) -> RawSocket {
        self.sys as RawSocket
    }
}

#[cfg(windows)]
impl FromRawSocket for TcpSocket {
    /// Converts a `RawSocket` to a `TcpSocket`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_socket(socket: RawSocket) -> TcpSocket {
        TcpSocket { sys: socket as sys::tcp::TcpSocket }
    }
}
