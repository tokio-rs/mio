use {NonBlock, IntoNonBlock, TryRead, TryWrite};
use buf::{Buf, MutBuf};
use io::{self, Evented, FromFd, Io};
use net::{self, nix, Socket};
use std::usize;
use std::iter::IntoIterator;
use std::path::Path;
use std::os::unix::io::{Fd, AsRawFd};

pub fn stream() -> io::Result<NonBlock<UnixSocket>> {
    UnixSocket::stream(true)
        .map(|sock| NonBlock::new(sock))
}

pub fn bind<P: AsRef<Path> + ?Sized>(addr: &P) -> io::Result<NonBlock<UnixListener>> {
    stream().and_then(|sock| {
        try!(sock.bind(addr));
        sock.listen(256)
    })
}

pub fn connect<P: AsRef<Path> + ?Sized>(addr: &P) -> io::Result<(NonBlock<UnixStream>, bool)> {
    stream().and_then(|sock| sock.connect(addr))
}

#[derive(Debug)]
pub struct UnixSocket {
    io: Io,
}

impl UnixSocket {
    fn stream(nonblock: bool) -> io::Result<UnixSocket> {
        UnixSocket::new(nix::SockType::Stream, nonblock)
    }

    fn new(ty: nix::SockType, nonblock: bool) -> io::Result<UnixSocket> {
        let fd = try!(net::socket(nix::AddressFamily::Unix, ty, nonblock));
        Ok(FromFd::from_fd(fd))
    }

    pub fn connect<P: AsRef<Path> + ?Sized>(self, addr: &P) -> io::Result<(UnixStream, bool)> {
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

    pub fn bind<P: AsRef<Path> + ?Sized>(&self, addr: &P) -> io::Result<()> {
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
    pub fn connect<P: AsRef<Path> + ?Sized>(self, addr: &P) -> io::Result<(NonBlock<UnixStream>, bool)> {
        self.unwrap().connect(addr)
            .map(|(stream, complete)| (NonBlock::new(stream), complete))
    }

    pub fn listen(self, backlog: usize) -> io::Result<NonBlock<UnixListener>> {
        self.unwrap().listen(backlog)
            .map(NonBlock::new)
    }
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
    pub fn listen<P: AsRef<Path> + ?Sized>(path: &P) -> io::Result<UnixListener> {
        UnixSocket::stream(false)
            .and_then(|sock| {
                try!(sock.bind(path));
                sock.listen(1024)
            })
    }

    pub fn accept(&self) -> io::Result<UnixStream> {
        net::accept(&self.io, false)
            .map(|fd| FromFd::from_fd(fd))
    }

    pub fn incoming<'a>(&'a self) -> Incoming<'a> {
        Incoming { listener: self }
    }
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
        net::accept(&self.io, true)
            .map(|fd| Some(NonBlock::new(FromFd::from_fd(fd))))
            .or_else(io::to_non_block)
    }
}

impl<'a> IntoIterator for &'a UnixListener {
    type Item = io::Result<UnixStream>;
    type IntoIter = Incoming<'a>;

    fn into_iter(self) -> Incoming<'a> {
        self.incoming()
    }
}

/// An iterator over incoming connections to a `UnixListener`.
///
/// It will never return `None`.
#[derive(Debug)]
pub struct Incoming<'a> {
    listener: &'a UnixListener,
}

impl<'a> Iterator for Incoming<'a> {
    type Item = io::Result<UnixStream>;

    fn next(&mut self) -> Option<io::Result<UnixStream>> {
        Some(self.listener.accept())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
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
        UnixSocket::stream(false)
            .and_then(|sock| sock.connect(path))
            .map(|(sock, _)| sock)
    }
}

impl io::Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.io.read_slice(buf) {
            Ok(Some(cnt)) => Ok(cnt),
            Ok(None) => Err(io::Error::from_os_error(nix::EAGAIN as i32)),
            Err(e) => Err(e),
        }
    }
}

impl io::Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.io.write_slice(buf) {
            Ok(Some(cnt)) => Ok(cnt),
            Ok(None) => Err(io::Error::from_os_error(nix::EAGAIN as i32)),
            Err(e) => Err(e),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
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

fn to_nix_addr<P: AsRef<Path> + ?Sized>(path: &P) -> io::Result<nix::SockAddr> {
    nix::SockAddr::new_unix(path.as_ref())
        .map_err(io::from_nix_error)
}
