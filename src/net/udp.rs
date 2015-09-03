use {io, sys, Evented, EventSet, IpAddr, PollOpt, Selector, Token};
use std::net::SocketAddr;

#[derive(Debug)]
pub struct UdpSocket {
    sys: sys::UdpSocket,
}

impl UdpSocket {
    /// Returns a new, unbound, non-blocking, IPv4 UDP socket
    pub fn v4() -> io::Result<UdpSocket> {
        sys::UdpSocket::v4()
            .map(From::from)
    }

    /// Returns a new, unbound, non-blocking, IPv6 UDP socket
    pub fn v6() -> io::Result<UdpSocket> {
        sys::UdpSocket::v6()
            .map(From::from)
    }

    pub fn bound(addr: &SocketAddr) -> io::Result<UdpSocket> {
        // Create the socket
        let sock = try!(match *addr {
            SocketAddr::V4(..) => UdpSocket::v4(),
            SocketAddr::V6(..) => UdpSocket::v6(),
        });

        // Bind the socket
        try!(sock.bind(addr));

        Ok(sock)
    }

    pub fn bind(&self, addr: &SocketAddr) -> io::Result<()> {
        self.sys.bind(addr)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sys.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<UdpSocket> {
        self.sys.try_clone()
            .map(From::from)
    }

    pub fn send_to(&self, buf: &[u8], target: &SocketAddr)
                   -> io::Result<Option<usize>> {
        self.sys.send_to(buf, target)
    }

    pub fn recv_from(&self, buf: &mut [u8])
                     -> io::Result<Option<(usize, SocketAddr)>> {
        self.sys.recv_from(buf)
    }

    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        self.sys.set_broadcast(on)
    }

    pub fn set_multicast_loop(&self, on: bool) -> io::Result<()> {
        self.sys.set_multicast_loop(on)
    }

    pub fn join_multicast(&self, multi: &IpAddr) -> io::Result<()> {
        self.sys.join_multicast(multi)
    }

    pub fn leave_multicast(&self, multi: &IpAddr) -> io::Result<()> {
        self.sys.leave_multicast(multi)
    }

    pub fn set_multicast_time_to_live(&self, ttl: i32) -> io::Result<()> {
        self.sys.set_multicast_time_to_live(ttl)
    }
}

impl Evented for UdpSocket {
    fn register(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sys.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sys.deregister(selector)
    }
}

impl From<sys::UdpSocket> for UdpSocket {
    fn from(sys: sys::UdpSocket) -> UdpSocket {
        UdpSocket { sys: sys }
    }
}

/*
 *
 * ===== UNIX ext =====
 *
 */

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

#[cfg(unix)]
impl AsRawFd for UdpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

#[cfg(unix)]
impl FromRawFd for UdpSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> UdpSocket {
        UdpSocket { sys: FromRawFd::from_raw_fd(fd) }
    }
}
