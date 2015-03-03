use {TryRead, TryWrite, NonBlock, MioResult, MioError};
use buf::{Buf, MutBuf};
use io::{self, Io, FromFd, IoHandle};
use net::{self, nix, TryAccept, Socket};
use std::path::Path;
use std::os::unix::Fd;

#[derive(Debug)]
pub struct UnixSocket {
    io: Io,
}

impl UnixSocket {
    pub fn stream() -> MioResult<UnixSocket> {
        UnixSocket::new(nix::SockType::Stream)
    }

    fn new(ty: nix::SockType) -> MioResult<UnixSocket> {
        let fd = try!(net::socket(nix::AddressFamily::Unix, ty));
        Ok(FromFd::from_fd(fd))
    }

    pub fn connect(self, addr: &Path) -> MioResult<(UnixStream, bool)> {
        let io = self.io;
        // Attempt establishing the context. This may not complete immediately.
        net::connect(&io, &try!(to_nix_addr(addr)))
            .map(|complete| (UnixStream { io: io }, complete))
    }

    pub fn listen(self, backlog: usize) -> MioResult<UnixListener> {
        let io = self.io;

        net::listen(&io, backlog)
            .map(|_| UnixListener { io: io })
    }

    pub fn bind(&self, addr: &Path) -> MioResult<()> {
        net::bind(&self.io, &try!(to_nix_addr(addr)))
    }
}

impl IoHandle for UnixSocket {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
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

impl IoHandle for UnixListener {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
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

    fn try_accept(&self) -> MioResult<NonBlock<UnixStream>> {
        net::accept(&self.io)
            .map(|fd| NonBlock::Ready(FromFd::from_fd(fd)))
            .or_else(io::to_non_block)
    }
}

#[derive(Debug)]
pub struct UnixStream {
    io: Io,
}

impl IoHandle for UnixStream {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
}

impl FromFd for UnixStream {
    fn from_fd(fd: Fd) -> UnixStream {
        UnixStream { io: Io::new(fd) }
    }
}

impl Socket for UnixStream {
}

impl TryRead for UnixStream {
    fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
        self.io.read_slice(buf)
    }
}

impl TryWrite for UnixStream {
    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
        self.io.write_slice(buf)
    }
}

fn to_nix_addr(path: &Path) -> MioResult<nix::SockAddr> {
    nix::SockAddr::new_unix(path)
        .map_err(MioError::from_nix_error)
}
