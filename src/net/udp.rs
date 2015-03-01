use buf::{Buf, MutBuf};
use error::MioResult;
use io::{self, Io, FromFd, IoHandle, IoReader, IoWriter, NonBlock};
use net::{self, nix, Socket, MulticastSocket, UnconnectedSocket};
use std::net::{SocketAddr, IpAddr};
use std::os::unix::Fd;

#[derive(Debug)]
pub struct UdpSocket {
    io: Io,
}

impl UdpSocket {
    pub fn v4() -> MioResult<UdpSocket> {
        UdpSocket::new(nix::AddressFamily::Inet)
    }

    pub fn v6() -> MioResult<UdpSocket> {
        UdpSocket::new(nix::AddressFamily::Inet6)
    }

    fn new(family: nix::AddressFamily) -> MioResult<UdpSocket> {
        let fd = try!(net::socket(family, nix::SockType::Datagram));
        Ok(FromFd::from_fd(fd))
    }

    pub fn bind(&self, addr: &SocketAddr) -> MioResult<()> {
        try!(net::bind(&self.io, &net::to_nix_addr(addr)));
        Ok(())
    }

    pub fn connect(&self, addr: &SocketAddr) -> MioResult<bool> {
        net::connect(&self.io, &net::to_nix_addr(addr))
    }

    pub fn bound(addr: &SocketAddr) -> MioResult<UdpSocket> {
        // Create the socket
        let sock = try!(match addr.ip() {
            IpAddr::V4(..) => UdpSocket::v4(),
            IpAddr::V6(..) => UdpSocket::v6(),
        });

        // Bind the socket
        try!(sock.bind(addr));

        // Return it
        Ok(sock)
    }
}

impl IoHandle for UdpSocket {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
}

impl FromFd for UdpSocket {
    fn from_fd(fd: Fd) -> UdpSocket {
        UdpSocket { io: Io::new(fd) }
    }
}

impl Socket for UdpSocket {
}

impl MulticastSocket for UdpSocket {
}

impl IoReader for UdpSocket {
    fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
        self.io.read_slice(buf)
    }
}

impl IoWriter for UdpSocket {
    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
        self.io.write_slice(buf)
    }
}

// Unconnected socket sender -- trait unique to sockets
impl UnconnectedSocket for UdpSocket {
    fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &SocketAddr) -> MioResult<NonBlock<()>> {
        net::sendto(&self.io, buf.bytes(), &net::to_nix_addr(tgt))
            .map(|cnt| {
                buf.advance(cnt);
                NonBlock::Ready(())
            })
            .or_else(io::to_non_block)
    }

    fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> MioResult<NonBlock<SocketAddr>> {
        net::recvfrom(&self.io, buf.mut_bytes())
            .map(|(cnt, addr)| {
                buf.advance(cnt);
                NonBlock::Ready(net::to_std_addr(addr))
            })
            .or_else(io::to_non_block)
    }
}
