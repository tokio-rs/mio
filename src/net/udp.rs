use {NonBlock, AsNonBlock, Io};
use buf::{Buf, MutBuf};
use io::{self, Evented, FromFd, Result};
use net::{self, nix, Socket};
use std::mem;
use std::net::{SocketAddr, IpAddr};
use std::os::unix::Fd;

pub use std::net::UdpSocket;

/// Returns a new, unbound, non-blocking, IPv4 UDP socket
pub fn v4() -> Result<NonBlock<UdpSocket>> {
    net::socket(nix::AddressFamily::Inet, nix::SockType::Datagram)
        .map(|fd| NonBlock::new(FromFd::from_fd(fd)))
}

/// Returns a new, unbound, non-blocking, IPv6 UDP socket
pub fn v6() -> Result<NonBlock<UdpSocket>> {
    net::socket(nix::AddressFamily::Inet6, nix::SockType::Datagram)
        .map(|fd| NonBlock::new(FromFd::from_fd(fd)))
}

/// Returns a new, non-blocking, UDP socket bound to the given address
pub fn bind(addr: &SocketAddr) -> io::Result<NonBlock<UdpSocket>> {
    let sock = match addr.ip() {
        IpAddr::V4(..) => try!(v4()),
        IpAddr::V6(..) => try!(v6()),
    };

    try!(sock.bind(addr));
    Ok(sock)
}

impl Evented for UdpSocket {
}

impl FromFd for UdpSocket {
    fn from_fd(fd: Fd) -> UdpSocket {
        unsafe { mem::transmute(Io::new(fd)) }
    }
}

impl Socket for UdpSocket {
}

impl AsNonBlock for UdpSocket {
    fn as_non_block(self) -> Result<NonBlock<UdpSocket>> {
        try!(net::set_non_block(as_io(&self)));
        Ok(NonBlock::new(self))
    }
}

impl NonBlock<UdpSocket> {
    pub fn bind(&self, addr: &SocketAddr) -> io::Result<()> {
        net::bind(as_io(&*self), &net::to_nix_addr(addr))
    }

    pub fn send_to<B: Buf>(&self, buf: &mut B, target: &SocketAddr) -> io::Result<Option<()>> {
        net::sendto(as_io(&*self), buf.bytes(), &net::to_nix_addr(target))
            .map(|cnt| {
                buf.advance(cnt);
                Some(())
            })
            .or_else(io::to_non_block)
    }

    pub fn recv_from<B: MutBuf>(&self, buf: &mut B) -> io::Result<Option<SocketAddr>> {
        net::recvfrom(as_io(&*self), buf.mut_bytes())
            .map(|(cnt, addr)| {
                buf.advance(cnt);
                Some(net::to_std_addr(addr))
            })
            .or_else(io::to_non_block)
    }
}

fn as_io<'a, T>(udp: &'a T) -> &'a Io {
    unsafe { mem::transmute(udp) }
}
