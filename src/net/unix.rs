use {NonBlock, IntoNonBlock, TryRead, TryWrite};
use buf::{Buf, MutBuf};
use io::{self, Evented, FromFd, Io};
use net::{self, nix, Socket};
use std::path::Path;
use std::os::unix::{Fd, AsRawFd};

pub fn stream() -> io::Result<NonBlock<UnixSocket>> {
    UnixSocket::stream()
        .map(|sock| NonBlock::new(sock))
}

pub fn bind(addr: &Path) -> io::Result<NonBlock<UnixListener>> {
    stream().and_then(|sock| {
        try!(sock.bind(addr));
        sock.listen(256)
    })
}

pub fn connect(addr: &Path) -> io::Result<(NonBlock<UnixStream>, bool)> {
    stream().and_then(|sock| sock.connect(addr))
}

#[derive(Debug)]
pub struct UnixSocket {
    io: Io,
}

impl UnixSocket {
    fn stream() -> io::Result<UnixSocket> {
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

impl IntoNonBlock for UnixSocket {
    fn into_non_block(self) -> io::Result<NonBlock<UnixSocket>> {
        try!(net::set_non_block(&self.io));
        Ok(NonBlock::new(self))
    }
}

impl NonBlock<UnixSocket> {
    pub fn connect(self, addr: &Path) -> io::Result<(NonBlock<UnixStream>, bool)> {
        self.unwrap().connect(addr)
            .map(|(stream, complete)| (NonBlock::new(stream), complete))
    }

    pub fn listen(self, backlog: usize) -> io::Result<NonBlock<UnixListener>> {
        self.unwrap().listen(backlog)
            .map(NonBlock::new)
    }
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

impl Socket for UnixListener {
}

impl IntoNonBlock for UnixListener {
    fn into_non_block(self) -> io::Result<NonBlock<UnixListener>> {
        try!(net::set_non_block(&self.io));
        Ok(NonBlock::new(self))
    }
}

impl FromFd for UnixListener {
    fn from_fd(fd: Fd) -> UnixListener {
        UnixListener { io: Io::new(fd) }
    }
}

impl NonBlock<UnixListener> {
    pub fn accept(&self) -> io::Result<Option<NonBlock<UnixStream>>> {
        net::accept(&self.io)
            .map(|fd| Some(NonBlock::new(FromFd::from_fd(fd))))
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

impl IntoNonBlock for UnixStream {
    fn into_non_block(self) -> io::Result<NonBlock<UnixStream>> {
        try!(net::set_non_block(&self.io));
        Ok(NonBlock::new(self))
    }
}

impl TryRead for NonBlock<UnixStream> {
    fn read_slice(&mut self, buf: &mut[u8]) -> io::Result<Option<usize>> {
        self.io.read_slice(buf)
    }
}

impl TryWrite for NonBlock<UnixStream> {
    fn write_slice(&mut self, buf: &[u8]) -> io::Result<Option<usize>> {
        self.io.write_slice(buf)
    }
}

fn to_nix_addr(path: &Path) -> io::Result<nix::SockAddr> {
    nix::SockAddr::new_unix(path)
        .map_err(io::from_nix_error)
}
