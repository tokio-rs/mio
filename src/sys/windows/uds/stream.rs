use super::socketaddr_un;
use super::startup;
use super::wsa_error;
use super::Socket;
use super::SocketAddr;
use std::fmt::Debug;
use std::io;
use std::net::Shutdown;
use std::os::windows::io::AsRawSocket;
use std::os::windows::io::RawSocket;
use std::path::Path;
use windows_sys::Win32::Networking::WinSock;
use windows_sys::Win32::Networking::WinSock::SOCKET_ERROR;
/// A Unix domain socket stream client.
///
/// This type represents a connected Unix domain socket client stream,
/// providing bidirectional I/O communication with a server.
///
/// # Examples
///
/// ```no_run
/// use std::io::{Read, Write};
///
/// let mut stream = UnixStream::connect("/tmp/socket.sock")?;
/// stream.write_all(b"Hello, server!")?;
///
/// let mut response = String::new();
/// stream.read_to_string(&mut response)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct UnixStream(Socket);
impl UnixStream {
    pub(crate) fn new(socket: Socket) -> Self {
        Self(socket)
    }
    /// Connects to a Unix domain socket server at the specified filesystem path.
    ///
    /// This function creates a new socket and establishes a connection to the server
    /// listening on the given path. The path must be a valid filesystem path that
    /// the server is bound to.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path of the server socket to connect to
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if:
    /// - Winsock initialization fails
    /// - Socket creation fails  
    /// - The connection attempt fails
    /// - The provided path is invalid
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let stream = UnixStream::connect("C:/my_socket")?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn connect<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        unsafe {
            startup()?;
            let s = Socket::new()?;
            let (addr, len) = socketaddr_un(path.as_ref())?;
            match WinSock::connect(s.0, &addr as *const _ as *const _, len) {
                SOCKET_ERROR => Err(wsa_error()),
                _ => Ok(Self(s)),
            }
        }
    }

    /// Connects to a Unix domain socket server using a pre-constructed `SocketAddr`.
    ///
    /// This function creates a new socket and establishes a connection to the server
    /// address specified by the given `SocketAddr`. This is useful when you already
    /// have a socket address constructed and want to reuse it.
    ///
    /// # Arguments
    ///
    /// * `socket_addr` - The socket address of the server to connect to
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if:
    /// - Socket creation fails
    /// - The connection attempt fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::sys::uds::SocketAddr;
    ///
    /// let addr = SocketAddr::from_path("C:/my_socket")?;
    /// let stream = UnixStream::connect_addr(&addr)?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn connect_addr(socket_addr: &SocketAddr) -> io::Result<Self> {
        let s = Socket::new()?;
        match unsafe {
            WinSock::connect(
                s.0,
                &socket_addr.addr as *const _ as *const _,
                socket_addr.addrlen,
            )
        } {
            SOCKET_ERROR => Err(wsa_error()),
            _ => Ok(Self(s)),
        }
    }
    /// Returns the socket address of the local half of this connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    /// Returns the socket address of the remote half of this connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.0.peer_addr()
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.take_error()
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.0.shutdown(how)
    }
    /// Sets the non-blocking mode for this socket
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }
}
impl io::Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut &*self, buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut &*self)
    }
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        io::Write::write_vectored(&mut &*self, bufs)
    }
}
impl io::Write for &UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(bufs)
    }
}
impl io::Read for &UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buf)
    }
    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        self.0.recv_vectored(bufs)
    }
}
impl io::Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut &*self, buf)
    }
    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        io::Read::read_vectored(&mut &*self, bufs)
    }
}

pub(crate) fn connect_addr(address: &SocketAddr) -> io::Result<UnixStream> {
    UnixStream::connect_addr(address)
}

impl AsRawSocket for UnixStream {
    fn as_raw_socket(&self) -> RawSocket {
        self.0 .0 as _
    }
}
