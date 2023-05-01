use std::io::{self, IoSlice, IoSliceMut};
use std::net::Shutdown;
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::{fmt, mem};

use windows_sys::Win32::Networking::WinSock::SOCKET_ERROR;

use super::{socket::Socket, SocketAddr};

/// A Unix stream socket.
pub struct UnixStream(pub(super) Socket);

impl UnixStream {
    /// Connects to the socket specified by [`address`].
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|addr, len| {
            wsa_syscall!(
                getsockname(self.0.as_raw_socket() as _, addr, len),
                SOCKET_ERROR
            )
        })
    }

    /// Returns the socket address of the remote half of this connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|addr, len| {
            wsa_syscall!(
                getpeername(self.0.as_raw_socket() as _, addr, len),
                SOCKET_ERROR
            )
        })
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.take_error()
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of [`Shutdown`]).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.0.shutdown(how)
    }
}

cfg_os_poll! {
    use std::path::Path;
    use super::socket_addr;

    impl UnixStream {
        /// Connects to the socket named by `path`.
        pub fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixStream> {
            let inner = Socket::new()?;
            let (addr, len) = socket_addr(path.as_ref())?;

            match wsa_syscall!(
                connect(
                    inner.as_raw_socket() as _,
                    &addr as *const _ as *const _,
                    len,
                ),
                SOCKET_ERROR
            ) {
                Ok(_) => {}
                Err(ref err) if err.kind() == std::io::ErrorKind::Other => {}
                Err(e) => return Err(e),
            }
            Ok(UnixStream(inner))
        }

        /// Moves the socket into or out of nonblocking mode.
        pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
            self.0.set_nonblocking(nonblocking)
        }
    }
}

impl fmt::Debug for UnixStream {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = fmt.debug_struct("UnixStream");
        builder.field("socket", &self.0.as_raw_socket());
        if let Ok(addr) = self.local_addr() {
            builder.field("local", &addr);
        }
        if let Ok(addr) = self.peer_addr() {
            builder.field("peer", &addr);
        }
        builder.finish()
    }
}

impl io::Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut &*self, buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        io::Read::read_vectored(&mut &*self, bufs)
    }
}

impl<'a> io::Read for &'a UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.0.recv_vectored(bufs)
    }
}

impl io::Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut &*self, buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        io::Write::write_vectored(&mut &*self, bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut &*self)
    }
}

impl<'a> io::Write for &'a UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsRawSocket for UnixStream {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}

impl FromRawSocket for UnixStream {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        UnixStream(Socket::from_raw_socket(sock))
    }
}

impl IntoRawSocket for UnixStream {
    fn into_raw_socket(self) -> RawSocket {
        let ret = self.0.as_raw_socket();
        mem::forget(self);
        ret
    }
}
