use buf::{Buf, MutBuf};
use error::MioResult;
use io::{self, FromIoDesc, IoHandle, IoReader, IoWriter, NonBlock};
use net::{self, nix, Socket, MulticastSocket, UnconnectedSocket};
use os;
use std::net::{SocketAddr, IpAddr};

#[derive(Debug)]
pub struct UdpSocket {
    desc: os::IoDesc
}

impl UdpSocket {
    pub fn v4() -> MioResult<UdpSocket> {
        UdpSocket::new(nix::AddressFamily::Inet)
    }

    pub fn v6() -> MioResult<UdpSocket> {
        UdpSocket::new(nix::AddressFamily::Inet6)
    }

    fn new(family: nix::AddressFamily) -> MioResult<UdpSocket> {
        Ok(UdpSocket { desc: try!(os::socket(family, nix::SockType::Datagram)) })
    }

    pub fn bind(&self, addr: &SocketAddr) -> MioResult<()> {
        try!(os::bind(&self.desc, &net::to_nix_addr(addr)));
        Ok(())
    }

    pub fn connect(&self, addr: &SocketAddr) -> MioResult<bool> {
        os::connect(&self.desc, &net::to_nix_addr(addr))
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
    fn desc(&self) -> &os::IoDesc {
        &self.desc
    }
}

impl FromIoDesc for UdpSocket {
    fn from_desc(desc: os::IoDesc) -> Self {
        UdpSocket { desc: desc }
    }
}

impl Socket for UdpSocket {
}

impl MulticastSocket for UdpSocket {
}

impl IoReader for UdpSocket {
    fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
        io::read(self, buf)
    }

    fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
        io::read_slice(self, buf)
    }
}

impl IoWriter for UdpSocket {
    fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
        io::write(self, buf)
    }

    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
        io::write_slice(self, buf)
    }
}

// Unconnected socket sender -- trait unique to sockets
impl UnconnectedSocket for UdpSocket {
    fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &SocketAddr) -> MioResult<NonBlock<()>> {
        match os::sendto(&self.desc, buf.bytes(), &net::to_nix_addr(tgt)) {
            Ok(cnt) => {
                buf.advance(cnt);
                Ok(NonBlock::Ready(()))
            }
            Err(e) => {
                if e.is_would_block() {
                    Ok(NonBlock::WouldBlock)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> MioResult<NonBlock<SocketAddr>> {
        match os::recvfrom(&self.desc, buf.mut_bytes()) {
            Ok((cnt, addr)) => {
                buf.advance(cnt);
                Ok(NonBlock::Ready(net::to_std_addr(addr)))
            }
            Err(e) => {
                if e.is_would_block() {
                    Ok(NonBlock::WouldBlock)
                } else {
                    Err(e)
                }
            }
        }
    }
}
