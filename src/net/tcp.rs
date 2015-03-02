use {TryRead, TryWrite, TryAccept, NonBlock, MioResult};
use buf::{Buf, MutBuf};
use io::{self, FromFd, Io, IoHandle};
use net::{self, nix, Socket};
use std::mem;
use std::net::SocketAddr;
use std::os::unix::{Fd, AsRawFd};

pub use std::net::{TcpStream, TcpListener};

/*
 *
 * ===== TcpSocket =====
 *
 */

#[derive(Debug)]
pub struct TcpSocket {
    io: Io,
}

impl TcpSocket {
    pub fn v4() -> MioResult<TcpSocket> {
        TcpSocket::new(nix::AddressFamily::Inet)
    }

    pub fn v6() -> MioResult<TcpSocket> {
        TcpSocket::new(nix::AddressFamily::Inet6)
    }

    fn new(family: nix::AddressFamily) -> MioResult<TcpSocket> {
        let fd = try!(net::socket(family, nix::SockType::Stream));
        Ok(FromFd::from_fd(fd))
    }

    pub fn connect(self, addr: &SocketAddr) -> MioResult<(TcpStream, bool)> {
        let io = self.io;
        // Attempt establishing the context. This may not complete immediately.
        net::connect(&io, &net::to_nix_addr(addr))
            .map(|complete| (to_tcp_stream(io), complete))
    }

    pub fn bind(&self, addr: &SocketAddr) -> MioResult<()> {
        net::bind(&self.io, &net::to_nix_addr(addr))
    }

    pub fn listen(self, backlog: usize) -> MioResult<TcpListener> {
        try!(net::listen(&self.io, backlog));
        Ok(to_tcp_listener(self.io))
    }

    pub fn getpeername(&self) -> MioResult<SocketAddr> {
        net::getpeername(&self.io)
            .map(net::to_std_addr)
    }

    pub fn getsockname(&self) -> MioResult<SocketAddr> {
        net::getsockname(&self.io)
            .map(net::to_std_addr)
    }
}

impl IoHandle for TcpSocket {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
}

impl FromFd for TcpSocket {
    fn from_fd(fd: Fd) -> TcpSocket {
        TcpSocket { io: Io::new(fd) }
    }
}

impl Socket for TcpSocket {
}

/*
 *
 * ===== TcpStream =====
 *
 */

impl FromFd for TcpStream {
    fn from_fd(fd: Fd) -> TcpStream {
        to_tcp_stream(Io::new(fd))
    }
}

impl IoHandle for TcpStream {
    fn fd(&self) -> Fd {
        self.as_raw_fd()
    }
}

impl Socket for TcpStream {
}

impl TryRead for TcpStream {
    fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
        as_io(self).read_slice(buf)
    }
}

impl TryWrite for TcpStream {
    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
        as_io(self).write_slice(buf)
    }
}

/*
 *
 * ===== TcpListener =====
 *
 */

impl FromFd for TcpListener {
    fn from_fd(fd: Fd) -> TcpListener {
        to_tcp_listener(Io::new(fd))
    }
}

impl IoHandle for TcpListener {
    fn fd(&self) -> Fd {
        self.as_raw_fd()
    }
}

impl Socket for TcpListener {
}

impl TryAccept for TcpListener {
    type Sock = TcpStream;

    fn try_accept(&self) -> MioResult<NonBlock<TcpStream>> {
        net::accept(as_io(self))
            .map(|fd| NonBlock::Ready(FromFd::from_fd(fd)))
            .or_else(io::to_non_block)
    }
}

/*
 *
 * ===== Conversions =====
 *
 */


fn to_tcp_stream(io: Io) -> TcpStream {
    unsafe { mem::transmute(io) }
}

fn to_tcp_listener(io: Io) -> TcpListener {
    unsafe { mem::transmute(io) }
}

fn as_io<'a, T>(tcp: &'a T) -> &'a Io {
    unsafe { mem::transmute(tcp) }
}
