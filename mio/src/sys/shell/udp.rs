use crate::{event, Interest, Registry, Token};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::{fmt, io, net};

pub struct UdpSocket {}

impl UdpSocket {
    pub fn bind(_: SocketAddr) -> io::Result<UdpSocket> {
        os_required!()
    }

    pub fn from_std(_: net::UdpSocket) -> UdpSocket {
        os_required!()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        os_required!()
    }

    pub fn try_clone(&self) -> io::Result<UdpSocket> {
        os_required!()
    }

    pub fn send_to(&self, _: &[u8], _: SocketAddr) -> io::Result<usize> {
        os_required!()
    }

    pub fn recv_from(&self, _: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        os_required!()
    }

    pub fn peek_from(&self, _: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        os_required!()
    }

    pub fn send(&self, _: &[u8]) -> io::Result<usize> {
        os_required!()
    }

    pub fn recv(&self, _: &mut [u8]) -> io::Result<usize> {
        os_required!()
    }

    pub fn peek(&self, _: &mut [u8]) -> io::Result<usize> {
        os_required!()
    }

    pub fn connect(&self, _: SocketAddr) -> io::Result<()> {
        os_required!()
    }

    pub fn broadcast(&self) -> io::Result<bool> {
        os_required!()
    }

    pub fn set_broadcast(&self, _: bool) -> io::Result<()> {
        os_required!()
    }

    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        os_required!()
    }

    pub fn set_multicast_loop_v4(&self, _: bool) -> io::Result<()> {
        os_required!()
    }

    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        os_required!()
    }

    pub fn set_multicast_ttl_v4(&self, _: u32) -> io::Result<()> {
        os_required!()
    }

    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        os_required!()
    }

    pub fn set_multicast_loop_v6(&self, _: bool) -> io::Result<()> {
        os_required!()
    }

    pub fn ttl(&self) -> io::Result<u32> {
        os_required!()
    }

    pub fn set_ttl(&self, _: u32) -> io::Result<()> {
        os_required!()
    }

    pub fn join_multicast_v4(&self, _: Ipv4Addr, _: Ipv4Addr) -> io::Result<()> {
        os_required!()
    }

    pub fn join_multicast_v6(&self, _: &Ipv6Addr, _: u32) -> io::Result<()> {
        os_required!()
    }

    pub fn leave_multicast_v4(&self, _: Ipv4Addr, _: Ipv4Addr) -> io::Result<()> {
        os_required!()
    }

    pub fn leave_multicast_v6(&self, _: &Ipv6Addr, _: u32) -> io::Result<()> {
        os_required!()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        os_required!()
    }
}

impl event::Source for UdpSocket {
    fn register(&mut self, _: &Registry, _: Token, _: Interest) -> io::Result<()> {
        os_required!()
    }

    fn reregister(&mut self, _: &Registry, _: Token, _: Interest) -> io::Result<()> {
        os_required!()
    }

    fn deregister(&mut self, _: &Registry) -> io::Result<()> {
        os_required!()
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        os_required!()
    }
}

#[cfg(unix)]
impl AsRawFd for UdpSocket {
    fn as_raw_fd(&self) -> RawFd {
        os_required!()
    }
}

#[cfg(unix)]
impl FromRawFd for UdpSocket {
    unsafe fn from_raw_fd(_: RawFd) -> UdpSocket {
        os_required!()
    }
}

#[cfg(unix)]
impl IntoRawFd for UdpSocket {
    fn into_raw_fd(self) -> RawFd {
        os_required!()
    }
}

#[cfg(windows)]
impl AsRawSocket for UdpSocket {
    fn as_raw_socket(&self) -> RawSocket {
        os_required!()
    }
}

#[cfg(windows)]
impl FromRawSocket for UdpSocket {
    unsafe fn from_raw_socket(_: RawSocket) -> UdpSocket {
        os_required!()
    }
}

#[cfg(windows)]
impl IntoRawSocket for UdpSocket {
    fn into_raw_socket(self) -> RawSocket {
        os_required!()
    }
}
