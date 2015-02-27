use os;
use error::MioResult;
use buf::{Buf, MutBuf};
use io;
use io::{FromIoDesc, IoHandle, IoAcceptor, IoReader, IoWriter, NonBlock};
use io::NonBlock::{Ready, WouldBlock};
use net::{self, nix, Socket};
use std::net::{SocketAddr, IpAddr};

#[derive(Debug)]
pub struct TcpSocket {
    desc: os::IoDesc
}

impl TcpSocket {
    pub fn v4() -> MioResult<TcpSocket> {
        TcpSocket::new(nix::AddressFamily::Inet)
    }

    pub fn v6() -> MioResult<TcpSocket> {
        TcpSocket::new(nix::AddressFamily::Inet6)
    }

    fn new(family: nix::AddressFamily) -> MioResult<TcpSocket> {
        Ok(TcpSocket { desc: try!(os::socket(family, nix::SockType::Stream)) })
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
        os::connect(&self.desc, &net::to_nix_addr(addr))
    }

    pub fn bind(self, addr: &SocketAddr) -> MioResult<TcpListener> {
        try!(os::bind(&self.desc, &net::to_nix_addr(addr)));
        Ok(TcpListener { desc: self.desc })
    }

    pub fn getpeername(&self) -> MioResult<SocketAddr> {
        os::getpeername(&self.desc)
            .map(net::to_std_addr)
    }

    pub fn getsockname(&self) -> MioResult<SocketAddr> {
        os::getsockname(&self.desc)
            .map(net::to_std_addr)
    }
}

impl IoHandle for TcpSocket {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
    }
}

impl FromIoDesc for TcpSocket {
    fn from_desc(desc: os::IoDesc) -> Self {
        TcpSocket { desc: desc }
    }
}

impl IoReader for TcpSocket {
    fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
        io::read(self, buf)
    }

    fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
        io::read_slice(self, buf)
    }
}

impl IoWriter for TcpSocket {
    fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
        io::write(self, buf)
    }

    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
        io::write_slice(self, buf)
    }
}

impl Socket for TcpSocket {
}

#[derive(Debug)]
pub struct TcpListener {
    desc: os::IoDesc,
}

impl TcpListener {
    pub fn listen(self, backlog: usize) -> MioResult<TcpAcceptor> {
        try!(os::listen(self.desc(), backlog));
        Ok(TcpAcceptor { desc: self.desc })
    }
}

impl IoHandle for TcpListener {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
    }
}

impl FromIoDesc for TcpListener {
    fn from_desc(desc: os::IoDesc) -> Self {
        TcpListener { desc: desc }
    }
}

#[derive(Debug)]
pub struct TcpAcceptor {
    desc: os::IoDesc,
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
}

impl IoHandle for TcpAcceptor {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
    }
}

impl FromIoDesc for TcpAcceptor {
    fn from_desc(desc: os::IoDesc) -> Self {
        TcpAcceptor { desc: desc }
    }
}

impl Socket for TcpAcceptor {
}

impl IoAcceptor for TcpAcceptor {
    type Output = TcpSocket;

    fn accept(&mut self) -> MioResult<NonBlock<TcpSocket>> {
        match os::accept(self.desc()) {
            Ok(sock) => Ok(Ready(TcpSocket { desc: sock })),
            Err(e) => {
                if e.is_would_block() {
                    return Ok(WouldBlock);
                }

                return Err(e);
            }
        }
    }
}
