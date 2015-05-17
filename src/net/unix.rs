use {TryRead, TryWrite};
use buf::{Buf, MutBuf};
use io::{self, Evented, FromFd, Io};
use net::{self, nix, Socket};
use std::path::Path;
use std::os::unix::io::{RawFd, AsRawFd};

#[derive(Debug)]
pub struct UnixSocket {
    io: Io,
}

impl UnixSocket {
    /// Returns a new, unbound, non-blocking Unix domain socket
    pub fn stream() -> io::Result<UnixSocket> {
        UnixSocket::new(nix::SockType::Stream)
    }

    fn new(ty: nix::SockType) -> io::Result<UnixSocket> {
        let fd = try!(net::socket(nix::AddressFamily::Unix, ty, true));
        Ok(FromFd::from_fd(fd))
    }

    /// Connect the socket to the specified address
    pub fn connect<P: AsRef<Path> + ?Sized>(self, addr: &P) -> io::Result<(UnixStream, bool)> {
        let io = self.io;
        // Attempt establishing the context. This may not complete immediately.
        net::connect(&io, &try!(to_nix_addr(addr)))
            .map(|complete| (UnixStream { io: io }, complete))
    }

    /// Listen for incoming requests
    pub fn listen(self, backlog: usize) -> io::Result<UnixListener> {
        let io = self.io;

        net::listen(&io, backlog)
            .map(|_| UnixListener { io: io })
    }

    /// Bind the socket to the specified address
    pub fn bind<P: AsRef<Path> + ?Sized>(&self, addr: &P) -> io::Result<()> {
        net::bind(&self.io, &try!(to_nix_addr(addr)))
    }

    pub fn try_clone(&self) -> io::Result<UnixSocket> {
        net::dup(&self.io)
            .map(|fd| UnixSocket { io: fd })
    }
}

impl AsRawFd for UnixSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl Evented for UnixSocket {
}

impl FromFd for UnixSocket {
    fn from_fd(fd: RawFd) -> UnixSocket {
        UnixSocket { io: Io::new(fd) }
    }
}

impl Socket for UnixSocket {
}

/*
 *
 * ===== UnixListener =====
 *
 */

#[derive(Debug)]
pub struct UnixListener {
    io: Io,
}

impl UnixListener {
    pub fn bind<P: AsRef<Path> + ?Sized>(addr: &P) -> io::Result<UnixListener> {
        UnixSocket::stream().and_then(|sock| {
            try!(sock.bind(addr));
            sock.listen(256)
        })
    }

    pub fn accept(&self) -> io::Result<Option<UnixStream>> {
        net::accept(&self.io, true)
            .map(|fd| Some(FromFd::from_fd(fd)))
            .or_else(io::to_non_block)
    }
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl Evented for UnixListener {
}

impl Socket for UnixListener {
}

impl FromFd for UnixListener {
    fn from_fd(fd: RawFd) -> UnixListener {
        UnixListener { io: Io::new(fd) }
    }
}

/*
 *
 * ===== UnixStream =====
 *
 */

#[derive(Debug)]
pub struct UnixStream {
    io: Io,
}

impl UnixStream {
    pub fn connect<P: AsRef<Path> + ?Sized>(path: &P) -> io::Result<UnixStream> {
        UnixSocket::stream()
            .and_then(|sock| sock.connect(path))
            .map(|(sock, _)| sock)
    }
}

impl io::TryRead for UnixStream {
    fn read_slice(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        self.io.read_slice(buf)
    }
}

impl io::TryWrite for UnixStream {
    fn write_slice(&mut self, buf: &[u8]) -> io::Result<Option<usize>> {
        self.io.write_slice(buf)
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl Evented for UnixStream {
}

impl FromFd for UnixStream {
    fn from_fd(fd: RawFd) -> UnixStream {
        UnixStream { io: Io::new(fd) }
    }
}

impl Socket for UnixStream {
}

fn to_nix_addr<P: AsRef<Path> + ?Sized>(path: &P) -> io::Result<nix::SockAddr> {
    nix::SockAddr::new_unix(path.as_ref())
        .map_err(io::from_nix_error)
}
