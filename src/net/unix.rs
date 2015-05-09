use {io, sys, Evented, Interest, Io, PollOpt, Selector, Token, TryRead, TryWrite};
use std::path::Path;

/// A trait to express the ability to construct an object from a raw file descriptor.
///
/// Once `std::os::unix::io::FromRawFd` is stable, this will go away
#[cfg(unix)]
pub trait FromRawFd {
    unsafe fn from_raw_fd(fd: RawFd) -> Self;
}

#[derive(Debug)]
pub struct UnixSocket {
    sys: sys::UnixSocket,
}

impl UnixSocket {
    /// Returns a new, unbound, non-blocking Unix domain socket
    pub fn stream() -> io::Result<UnixSocket> {
        sys::UnixSocket::stream()
            .map(From::from)
    }

    /// Connect the socket to the specified address
    pub fn connect<P: AsRef<Path> + ?Sized>(self, addr: &P) -> io::Result<(UnixStream, bool)> {
        let complete = try!(self.sys.connect(addr));
        Ok((From::from(self.sys), complete))
    }

    /// Bind the socket to the specified address
    pub fn bind<P: AsRef<Path> + ?Sized>(&self, addr: &P) -> io::Result<()> {
        self.sys.bind(addr)
    }

    /// Listen for incoming requests
    pub fn listen(self, backlog: usize) -> io::Result<UnixListener> {
        try!(self.sys.listen(backlog));
        Ok(From::from(self.sys))
    }

    pub fn try_clone(&self) -> io::Result<UnixSocket> {
        self.sys.try_clone()
            .map(From::from)
    }
}

impl Evented for UnixSocket {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sys.deregister(selector)
    }
}

impl From<sys::UnixSocket> for UnixSocket {
    fn from(sys: sys::UnixSocket) -> UnixSocket {
        UnixSocket { sys: sys }
    }
}

/*
 *
 * ===== UnixStream =====
 *
 */

#[derive(Debug)]
pub struct UnixStream {
    sys: sys::UnixSocket,
}

impl UnixStream {
    pub fn connect<P: AsRef<Path> + ?Sized>(path: &P) -> io::Result<UnixStream> {
        UnixSocket::stream()
            .and_then(|sock| sock.connect(path))
            .map(|(sock, _)| sock)
    }

    pub fn try_clone(&self) -> io::Result<UnixStream> {
        self.sys.try_clone()
            .map(From::from)
    }
}

impl io::TryRead for UnixStream {
    fn read_slice(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        self.sys.read_slice(buf)
    }
}

impl io::TryWrite for UnixStream {
    fn write_slice(&mut self, buf: &[u8]) -> io::Result<Option<usize>> {
        self.sys.write_slice(buf)
    }
}

impl Evented for UnixStream {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sys.deregister(selector)
    }
}

impl From<sys::UnixSocket> for UnixStream {
    fn from(sys: sys::UnixSocket) -> UnixStream {
        UnixStream { sys: sys }
    }
}

/*
 *
 * ===== UnixListener =====
 *
 */

#[derive(Debug)]
pub struct UnixListener {
    sys: sys::UnixSocket,
}

impl UnixListener {
    pub fn bind<P: AsRef<Path> + ?Sized>(addr: &P) -> io::Result<UnixListener> {
        UnixSocket::stream().and_then(|sock| {
            try!(sock.bind(addr));
            sock.listen(256)
        })
    }

    pub fn accept(&self) -> io::Result<Option<UnixStream>> {
        self.sys.accept()
            .map(|opt| opt.map(From::from))
    }

    pub fn try_clone(&self) -> io::Result<UnixListener> {
        self.sys.try_clone()
            .map(From::from)
    }
}

impl Evented for UnixListener {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sys.deregister(selector)
    }
}

impl From<sys::UnixSocket> for UnixListener {
    fn from(sys: sys::UnixSocket) -> UnixListener {
        UnixListener { sys: sys }
    }
}

/*
 *
 * ===== Pipe =====
 *
 */

pub fn pipe() -> io::Result<(PipeReader, PipeWriter)> {
    let (rd, wr) = try!(sys::pipe());
    Ok((From::from(rd), From::from(wr)))
}

pub struct PipeReader {
    io: Io,
}

impl TryRead for PipeReader {
    fn read_slice(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        self.io.read_slice(buf)
    }
}

impl Evented for PipeReader {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.io.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.io.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.io.deregister(selector)
    }
}

impl From<Io> for PipeReader {
    fn from(io: Io) -> PipeReader {
        PipeReader { io: io }
    }
}

pub struct PipeWriter {
    io: Io,
}

impl TryWrite for PipeWriter {
    fn write_slice(&mut self, buf: &[u8]) -> io::Result<Option<usize>> {
        self.io.write_slice(buf)
    }
}

impl Evented for PipeWriter {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.io.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.io.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.io.deregister(selector)
    }
}

impl From<Io> for PipeWriter {
    fn from(io: Io) -> PipeWriter {
        PipeWriter { io: io }
    }
}

/*
 *
 * ===== Conversions =====
 *
 */

use std::os::unix::io::{RawFd, AsRawFd};

impl AsRawFd for UnixSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

impl FromRawFd for UnixSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixSocket {
        UnixSocket { sys: FromRawFd::from_raw_fd(fd) }
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

impl FromRawFd for UnixStream {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixStream {
        UnixStream { sys: FromRawFd::from_raw_fd(fd) }
    }
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

impl FromRawFd for UnixListener {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixListener {
        UnixListener { sys: FromRawFd::from_raw_fd(fd) }
    }
}

impl AsRawFd for PipeReader {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl FromRawFd for PipeReader {
    unsafe fn from_raw_fd(fd: RawFd) -> PipeReader {
        PipeReader { io: FromRawFd::from_raw_fd(fd) }
    }
}

impl AsRawFd for PipeWriter {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl FromRawFd for PipeWriter {
    unsafe fn from_raw_fd(fd: RawFd) -> PipeWriter {
        PipeWriter { io: FromRawFd::from_raw_fd(fd) }
    }
}
