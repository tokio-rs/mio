use {TryRead, TryWrite};
use io::{self, Evented, FromFd, Io};
use net::{self, nix, Socket};
use std::net::SocketAddr;
use std::os::unix::io::{RawFd, AsRawFd};

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
            .map(FromFd::from_fd)
    }

    pub fn connect(self, addr: &SocketAddr) -> io::Result<(TcpStream, bool)> {
        let complete = try!(net::connect(&self.io, &net::to_nix_addr(addr)));
        Ok((TcpStream { io: self.io }, complete))
    }

    pub fn bind(&self, addr: &SocketAddr) -> io::Result<()> {
        net::bind(&self.io, &net::to_nix_addr(addr))
    }

    pub fn listen(self, backlog: usize) -> io::Result<TcpListener> {
        try!(net::listen(&self.io, backlog));
        Ok(TcpListener { io: self.io })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        net::getpeername(&self.io)
            .map(net::to_std_addr)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        net::getsockname(&self.io)
            .map(net::to_std_addr)
    }
}

impl Evented for TcpSocket {
}

impl AsRawFd for TcpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl FromFd for TcpSocket {
    fn from_fd(fd: RawFd) -> TcpSocket {
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

pub struct TcpStream {
    io: Io,
}

impl TcpStream {
    pub fn connect(addr: &SocketAddr) -> io::Result<TcpStream> {
        let sock = try!(match *addr {
            SocketAddr::V4(..) => TcpSocket::v4(),
            SocketAddr::V6(..) => TcpSocket::v6(),
        });

        sock.connect(addr)
            .map(|(stream, _)| stream)
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        net::getpeername(&self.io)
            .map(net::to_std_addr)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        net::getsockname(&self.io)
            .map(net::to_std_addr)
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        net::dup(&self.io)
            .map(|fd| TcpStream { io: fd })
    }
}

impl TryRead for TcpStream {
    fn read_slice(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        self.io.read_slice(buf)
    }
}

impl TryWrite for TcpStream {
    fn write_slice(&mut self, buf: &[u8]) -> io::Result<Option<usize>> {
        self.io.write_slice(buf)
    }
}

impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl FromFd for TcpStream {
    fn from_fd(fd: RawFd) -> TcpStream {
        TcpStream { io: Io::new(fd) }
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

pub struct TcpListener {
    io: Io,
}

impl TcpListener {
    pub fn bind(addr: &SocketAddr) -> io::Result<TcpListener> {
        // Create the socket
        let sock = try!(match *addr {
            SocketAddr::V4(..) => TcpSocket::v4(),
            SocketAddr::V6(..) => TcpSocket::v6(),
        });

        // Bind the socket
        try!(sock.bind(addr));

        // listen
        sock.listen(1024)
    }

    /// Accepts a new `TcpStream`.
    ///
    /// Returns a `Ok(None)` when the socket `WOULDBLOCK`, this means the stream will be ready at
    /// a later point.
    pub fn accept(&self) -> io::Result<Option<TcpStream>> {
        net::accept(&self.io, true)
            .map(|fd| Some(FromFd::from_fd(fd)))
            .or_else(io::to_non_block)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        net::getsockname(&self.io)
            .map(net::to_std_addr)
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        unimplemented!();
    }
}

impl FromFd for TcpListener {
    fn from_fd(fd: RawFd) -> TcpListener {
        TcpListener { io: Io::new(fd) }
    }
}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

impl Evented for TcpListener {
}

impl Socket for TcpListener {
}
