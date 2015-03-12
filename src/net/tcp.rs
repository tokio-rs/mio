use {NonBlock};
use io::{self, Evented, FromFd, Io};
use net::{self, nix, Socket};
use std::mem;
use std::net::{SocketAddr, IpAddr};
use std::os::unix::{Fd, AsRawFd};

pub use std::net::{TcpStream, TcpListener};

pub fn v4() -> io::Result<NonBlock<TcpSocket>> {
    TcpSocket::new(nix::AddressFamily::Inet, true)
        .map(NonBlock::new)
}

pub fn v6() -> io::Result<NonBlock<TcpSocket>> {
    TcpSocket::new(nix::AddressFamily::Inet6, true)
        .map(NonBlock::new)
}

pub fn listen(addr: &SocketAddr) -> io::Result<NonBlock<TcpListener>> {
    // Create the socket
    let sock = try!(match addr.ip() {
        IpAddr::V4(..) => v4(),
        IpAddr::V6(..) => v6(),
    });

    // Bind the socket
    try!(sock.bind(addr));

    // listen
    sock.listen(1024)
}

pub fn connect(addr: &SocketAddr) -> io::Result<(NonBlock<TcpStream>, bool)> {
    let sock = try!(match addr.ip() {
        IpAddr::V4(..) => v4(),
        IpAddr::V6(..) => v6(),
    });

    sock.connect(addr)
}

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
    fn new(family: nix::AddressFamily, nonblock: bool) -> io::Result<TcpSocket> {
        net::socket(family, nix::SockType::Stream, nonblock)
            .map(FromFd::from_fd)
    }

    pub fn connect(self, addr: &SocketAddr) -> io::Result<(TcpStream, bool)> {
        let io = self.io;
        // Attempt establishing the context. This may not complete immediately.
        net::connect(&io, &net::to_nix_addr(addr))
            .map(|complete| (to_tcp_stream(io), complete))
    }

    pub fn bind(&self, addr: &SocketAddr) -> io::Result<()> {
        net::bind(&self.io, &net::to_nix_addr(addr))
    }

    pub fn listen(self, backlog: usize) -> io::Result<TcpListener> {
        try!(net::listen(&self.io, backlog));
        Ok(to_tcp_listener(self.io))
    }

    pub fn getpeername(&self) -> io::Result<SocketAddr> {
        net::getpeername(&self.io)
            .map(net::to_std_addr)
    }

    pub fn getsockname(&self) -> io::Result<SocketAddr> {
        net::getsockname(&self.io)
            .map(net::to_std_addr)
    }
}

impl NonBlock<TcpSocket> {
    pub fn listen(self, backlog: usize) -> io::Result<NonBlock<TcpListener>> {
        self.unwrap().listen(backlog)
            .map(NonBlock::new)
    }

    pub fn connect(self, addr: &SocketAddr) -> io::Result<(NonBlock<TcpStream>, bool)> {
        self.unwrap().connect(addr)
            .map(|(stream, complete)| (NonBlock::new(stream), complete))
    }
}

impl Evented for TcpSocket {
}

impl AsRawFd for TcpSocket {
    fn as_raw_fd(&self) -> Fd {
        self.io.as_raw_fd()
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

impl Evented for TcpStream {
}

impl Socket for TcpStream {
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

impl Evented for TcpListener {
}

impl Socket for TcpListener {
}

impl NonBlock<TcpListener> {
    pub fn accept(&self) -> io::Result<Option<NonBlock<TcpStream>>> {
        net::accept(as_io(self), true)
            .map(|fd| Some(FromFd::from_fd(fd)))
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
