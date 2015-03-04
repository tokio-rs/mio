use {TryRead, TryWrite};
use buf::{Buf, MutBuf};
use io::{self, Evented, FromFd, Io};
use net::{self, nix, TryAccept, Socket};
use std::path::Path;
use std::os::unix::{Fd, AsRawFd};

#[derive(Debug)]
pub struct UnixSocket {
    io: Io,
}

impl UnixSocket {
    pub fn stream() -> io::Result<UnixSocket> {
        UnixSocket::new(nix::SockType::Stream)
    }

    fn new(ty: nix::SockType) -> io::Result<UnixSocket> {
        let fd = try!(net::socket(nix::AddressFamily::Unix, ty));
        Ok(FromFd::from_fd(fd))
    }

    pub fn connect(self, addr: &Path) -> io::Result<(UnixStream, bool)> {
        let io = self.io;
        // Attempt establishing the context. This may not complete immediately.
        net::connect(&io, &try!(to_nix_addr(addr)))
            .map(|complete| (UnixStream { io: io }, complete))
    }

    pub fn listen(self, backlog: usize) -> io::Result<UnixListener> {
        let io = self.io;

        net::listen(&io, backlog)
            .map(|_| UnixListener { io: io })
    }

    pub fn bind(&self, addr: &Path) -> io::Result<()> {
        net::bind(&self.io, &try!(to_nix_addr(addr)))
    }
}

impl AsRawFd for UnixSocket {
    fn as_raw_fd(&self) -> Fd {
        self.io.as_raw_fd()
    }
}

impl Evented for UnixSocket {
}

impl FromFd for UnixSocket {
    fn from_fd(fd: Fd) -> UnixSocket {
        UnixSocket { io: Io::new(fd) }
    }
}

impl Socket for UnixSocket {
}

#[derive(Debug)]
pub struct UnixListener {
    io: Io,
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> Fd {
        self.io.as_raw_fd()
    }
}

impl Evented for UnixListener {
}

impl FromFd for UnixListener {
    fn from_fd(fd: Fd) -> UnixListener {
        UnixListener { io: Io::new(fd) }
    }
}

impl Socket for UnixListener {
}

impl TryAccept for UnixListener {
    type Sock = UnixStream;

    fn try_accept(&self) -> io::Result<Option<UnixStream>> {
        net::accept(&self.io)
            .map(|fd| Some(FromFd::from_fd(fd)))
            .or_else(io::to_non_block)
    }
}

#[derive(Debug)]
pub struct UnixStream {
    io: Io,
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> Fd {
        self.io.as_raw_fd()
    }
}

impl Evented for UnixStream {
}

impl FromFd for UnixStream {
    fn from_fd(fd: Fd) -> UnixStream {
        UnixStream { io: Io::new(fd) }
    }
}

impl Socket for UnixStream {
}

impl TryRead for UnixStream {
    fn read_slice(&mut self, buf: &mut[u8]) -> io::Result<Option<usize>> {
        self.io.read_slice(buf)
    }
}

impl TryWrite for UnixStream {
    fn write_slice(&mut self, buf: &[u8]) -> io::Result<Option<usize>> {
        self.io.write_slice(buf)
    }
}

fn to_nix_addr(path: &Path) -> io::Result<nix::SockAddr> {
    nix::SockAddr::new_unix(path)
        .map_err(io::from_nix_error)
}
