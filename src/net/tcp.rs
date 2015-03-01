use {TryRead, TryWrite, NonBlock, MioResult};
use buf::{Buf, MutBuf};
use io::{self, FromFd, Io, IoHandle};
use net::{self, nix, Socket};
use std::net::{SocketAddr, IpAddr};
use std::os::unix::Fd;

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

    /// Connects the socket to the specified address. When the operation
    /// completes, the handler will be notified with the supplied token.
    ///
    /// The goal of this method is to ensure that the event loop will always
    /// notify about the connection, even if the connection happens
    /// immediately. Otherwise, every consumer of the event loop would have
    /// to worry about possibly-immediate connection.
    pub fn connect(&self, addr: &SocketAddr) -> MioResult<bool> {
        // Attempt establishing the context. This may not complete immediately.
        net::connect(&self.io, &net::to_nix_addr(addr))
    }

    pub fn bind(self, addr: &SocketAddr) -> MioResult<TcpListener> {
        try!(net::bind(&self.io, &net::to_nix_addr(addr)));
        Ok(TcpListener { io: self.io })
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

impl TryRead for TcpSocket {
    fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
        self.io.read_slice(buf)
    }
}

impl TryWrite for TcpSocket {
    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
        self.io.write_slice(buf)
    }
}

impl Socket for TcpSocket {
}

#[derive(Debug)]
pub struct TcpListener {
    io: Io,
}

impl TcpListener {
    pub fn listen(self, backlog: usize) -> MioResult<TcpAcceptor> {
        try!(net::listen(&self.io, backlog));
        Ok(TcpAcceptor { io: self.io })
    }
}

impl IoHandle for TcpListener {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
}

impl FromFd for TcpListener {
    fn from_fd(fd: Fd) -> TcpListener {
        TcpListener { io: Io::new(fd) }
    }
}

#[derive(Debug)]
pub struct TcpAcceptor {
    io: Io,
}

impl TcpAcceptor {
    pub fn new(addr: &SocketAddr, backlog: usize) -> MioResult<TcpAcceptor> {
        // Create the socket
        let sock = try!(match addr.ip() {
            IpAddr::V4(..) => TcpSocket::v4(),
            IpAddr::V6(..) => TcpSocket::v6(),
        });

        // Bind the socket
        let listener = try!(sock.bind(addr));

        // Start listening
        listener.listen(backlog)
    }

    pub fn accept(&mut self) -> MioResult<NonBlock<TcpSocket>> {
        net::accept(&self.io)
            .map(|fd| NonBlock::Ready(FromFd::from_fd(fd)))
            .or_else(io::to_non_block)
    }
}

impl IoHandle for TcpAcceptor {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
}

impl FromFd for TcpAcceptor {
    fn from_fd(fd: Fd) -> TcpAcceptor {
        TcpAcceptor { io: Io::new(fd) }
    }
}

impl Socket for TcpAcceptor {
}
