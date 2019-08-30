use crate::{Interests, Registry, Token, sys};
use crate::event::Source;
use crate::unix::SourceFd;

use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::SocketAddr;
use std::path::Path;

/// A non-blocking Unix stream socket.
#[derive(Debug)]
pub struct UnixStream {
    std: std::os::unix::net::UnixStream,
}

impl UnixStream {
    /// Connects to the socket named by `path`.
    pub fn connect<P: AsRef<Path>>(p: P) -> io::Result<UnixStream> {
        let std = sys::uds::connect_stream(p.as_ref())?;
        Ok(UnixStream { std })
    }

    /// Converts a `std` `UnixStream` to a `mio` `UnixStream`.
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    pub fn from_std(std: std::os::unix::net::UnixStream) -> UnixStream {
        UnixStream { std }
    }

    /// Creates an unnamed pair of connected sockets.
    ///
    /// Returns two `UnixStream`s which are connected to each other.
    pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
        let (a, b) = sys::uds::pair_stream()?;
        Ok((UnixStream::from_std(a), UnixStream::from_std(b)))
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `UnixStream` is a reference to the same stream that this
    /// object references. Both handles will read and write the same stream of
    /// data, and options set on one stream will be propogated to the other
    /// stream.
    pub fn try_clone(&self) -> io::Result<UnixStream> {
        self.std.try_clone().map(|std| {
            UnixStream { std }
        })
    }

    /// Returns the socket address of the local half of this connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.std.local_addr()
    }

    /// Returns the socket address of the remote half of this connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.std.peer_addr()
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.std.take_error()
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.std.shutdown(how)
    }
}

impl Source for UnixStream {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).register(registry, token, interests)
    }

    fn reregister(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).deregister(registry)
    }
}

impl io::Read for UnixStream {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        self.std.read(dst)
    }
}

impl<'a> io::Read for &'a UnixStream {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        (&self.std).read(dst)
    }
}

impl io::Write for UnixStream {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        self.std.write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.std.flush()
    }
}

impl<'a> io::Write for &'a UnixStream {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        (&self.std).write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.std).flush()
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.std.as_raw_fd()
    }
}

impl IntoRawFd for UnixStream {
    fn into_raw_fd(self) -> RawFd {
        self.std.into_raw_fd()
    }
}

impl FromRawFd for UnixStream {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixStream {
        let std = std::os::unix::net::UnixStream::from_raw_fd(fd);
        UnixStream { std }
    }
}
