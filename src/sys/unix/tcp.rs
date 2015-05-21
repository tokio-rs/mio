use {io, Evented, Interest, Io, PollOpt, Selector, Token};
use unix::FromRawFd;
use sys::unix::{net, nix, Socket};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::os::unix::io::{RawFd, AsRawFd};

#[derive(Debug)]
pub struct TcpSocket {
    io: Io,
}

impl TcpSocket {
    /// Returns a new, unbound, non-blocking, IPv4 socket
    pub fn v4() -> io::Result<TcpSocket> {
        TcpSocket::new(nix::AddressFamily::Inet)
    }

    /// Returns a new, unbound, non-blocking, IPv6 socket
    pub fn v6() -> io::Result<TcpSocket> {
        TcpSocket::new(nix::AddressFamily::Inet6)
    }

    fn new(family: nix::AddressFamily) -> io::Result<TcpSocket> {
        net::socket(family, nix::SockType::Stream, true)
            .map(|fd| From::from(Io::from_raw_fd(fd)))
    }

    pub fn connect(&self, addr: &SocketAddr) -> io::Result<bool> {
        net::connect(&self.io, &net::to_nix_addr(addr))
    }

    pub fn bind(&self, addr: &SocketAddr) -> io::Result<()> {
        net::bind(&self.io, &net::to_nix_addr(addr))
    }

    pub fn listen(&self, backlog: usize) -> io::Result<()> {
        net::listen(&self.io, backlog)
    }

    pub fn accept(&self) -> io::Result<Option<TcpSocket>> {
        net::accept(&self.io, true)
            .map(|fd| Some(From::from(Io::from_raw_fd(fd))))
            .or_else(io::to_non_block)
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        net::getpeername(&self.io)
            .map(net::to_std_addr)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        net::getsockname(&self.io)
            .map(net::to_std_addr)
    }

    pub fn try_clone(&self) -> io::Result<TcpSocket> {
        net::dup(&self.io)
            .map(From::from)
    }

    /*
     *
     * ===== Socket Options =====
     *
     */

    pub fn set_reuseaddr(&self, val: bool) -> io::Result<()> {
        Socket::set_reuseaddr(self, val)
    }
}

impl Read for TcpSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.read(buf)
    }
}

impl Write for TcpSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.io.flush()
    }
}

impl Evented for TcpSocket {
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

impl Socket for TcpSocket {
}

impl From<Io> for TcpSocket {
    fn from(io: Io) -> TcpSocket {
        TcpSocket { io: io }
    }
}

impl FromRawFd for TcpSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpSocket {
        TcpSocket { io: Io::from_raw_fd(fd) }
    }
}

impl AsRawFd for TcpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}
