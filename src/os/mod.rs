use {io, Listenable, PipeReader, PipeWriter};
use std::mem;
use std::num::Int;
use std::io::Result;
use std::os::unix::Fd;

#[cfg(target_os = "linux")]
pub use self::epoll::{Events, Selector};

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use self::kqueue::{Events, Selector};

#[cfg(target_os = "linux")]
pub use self::linux::Awakener;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use self::PipeAwakener as Awakener;

#[cfg(target_os = "linux")]
mod epoll;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod kqueue;

#[cfg(target_os = "linux")]
mod linux;

mod nix {
    pub use nix::{c_int, NixError};
    pub use nix::fcntl::{Fd, O_NONBLOCK, O_CLOEXEC};
    pub use nix::errno::EINPROGRESS;
    pub use nix::sys::socket::*;
    pub use nix::unistd::*;
}

pub trait FromFd {
    fn from_fd(fd: Fd) -> Self;
}

/*
 *
 * ===== Addresses =====
 *
 * TODO:
 *  - Move to nix?
 *
 */

pub use std::net::SocketAddr as InetAddr;
pub use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Copy)]
pub enum AddressFamily {
    Inet,
    Inet6,
    Unix,
}

#[derive(Copy)]
pub enum SocketType {
    Dgram,
    Stream,
}

/*
 *
 * ===== Awakener =====
 *
 */

pub struct PipeAwakener {
    reader: PipeReader,
    writer: PipeWriter
}

impl PipeAwakener {
    pub fn new() -> Result<PipeAwakener> {
        let (rd, wr) = try!(::pipe());

        Ok(PipeAwakener {
            reader: rd,
            writer: wr
        })
    }

    pub fn wakeup(&self) -> Result<()> {
        write(&self.writer, b"0x01")
            .map(|_| ())
    }

    pub fn cleanup(&self) {
        let mut buf: [u8; 128] = unsafe { mem::uninitialized() };

        loop {
            // Consume data until all bytes are purged
            match read(&self.reader, buf.as_mut_slice()) {
                Ok(_) => {}
                Err(_) => return
            }
        }
    }
}

impl Listenable for PipeAwakener {
    fn as_fd(&self) -> Fd {
        self.reader.as_fd()
    }
}

/*
 *
 * ===== Pipes =====
 *
 */

pub fn pipe() -> Result<(Fd, Fd)> {
    nix::pipe2(nix::O_NONBLOCK | nix::O_CLOEXEC)
        .map_err(io::to_io_error)
}

/*
 *
 * ===== Sockets =====
 *
 */

/// Create a socket with the given arguments
pub fn socket(af: AddressFamily, sock_type: SocketType) -> Result<Fd> {
    let family = match af {
        Inet  => nix::AF_INET,
        Inet6 => nix::AF_INET6,
        Unix  => nix::AF_UNIX
    };

    let socket_type = match sock_type {
        Dgram  => nix::SOCK_DGRAM,
        Stream => nix::SOCK_STREAM
    };

    nix::socket(family, socket_type, nix::SOCK_NONBLOCK | nix::SOCK_CLOEXEC)
        .map_err(io::to_io_error)
}

pub fn connect<A: nix::ToSockAddr>(fd: Fd, addr: &A) -> Result<bool> {
    nix::connect(fd, addr)
        .map(|_| true)
        .or_else(|e| {
            match e {
                nix::NixError::Sys(nix::EINPROGRESS) => Ok(false),
                _ => Err(io::to_io_error)
            }
        })
}

pub fn bind<A: nix::ToSockAddr>(fd: Fd, addr: &A) -> Result<()> {
    nix::bind(fd, addr)
        .map_err(io::to_io_error)
}

pub fn listen(fd: Fd, backlog: usize) -> Result<()> {
    nix::listen(fd, backlog)
        .map_err(io::to_io_error)
}

pub fn accept(fd: Fd) -> Result<Fd> {
    nix::accept4(fd, nix::SOCK_NONBLOCK | nix::SOCK_CLOEXEC)
        .map_err(io::to_io_error)
}

pub fn recvfrom(fd: Fd, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
    match nix::recvfrom(fd, buf).map_err(io::to_io_error) {
        Ok((cnt, addr)) => Ok((cnt, to_sockaddr(&addr))),
        Err(e) => Err(e)
    }
}

/// Send to a file descriptor
pub fn sendto(fd: Fd, buf: &[u8], tgt: &SocketAddr) -> Result<usize> {
    let res = try!(nix::sendto(fd, buf, &from_sockaddr(tgt), nix::MSG_DONTWAIT).map_err(io::to_io_error));
    Ok(res)
}

/*
 *
 * ===== Read / Write / Close =====
 *
 */

/// Read from a file descriptor
pub fn read(fd: Fd, dst: &mut [u8]) -> Result<usize> {
    nix::read(fd, dst).map_err(io::to_io_error)
}

/// Write to a file descriptor
pub fn write(fd: Fd, src: &[u8]) -> Result<usize> {
    nix::write(fd, src).map_err(io::to_io_error)
}

/// Close a file descriptor
pub fn close(fd: Fd) -> Result<()> {
    nix::close(fd).map_err(io::to_io_error)
}

/*
 *
 * ===== Socket options =====
 *
 */

pub fn reuseaddr(_fd: Fd) -> Result<usize> {
    unimplemented!()
}

pub fn set_reuseaddr(fd: Fd, val: bool) -> Result<()> {
    let v: nix::c_int = if val { 1 } else { 0 };

    nix::setsockopt(fd, nix::SOL_SOCKET, nix::SO_REUSEADDR, &v)
        .map_err(io::to_io_error)
}

pub fn set_reuseport(fd: Fd, val: bool) -> Result<()> {
    let v: nix::c_int = if val { 1 } else { 0 };

    nix::setsockopt(fd, nix::SOL_SOCKET, nix::SO_REUSEPORT, &v)
        .map_err(io::to_io_error)
}

pub fn set_tcp_nodelay(fd: Fd, val: bool) -> Result<()> {
    let v: nix::c_int = if val { 1 } else { 0 };

    nix::setsockopt(fd, nix::IPPROTO_TCP, nix::TCP_NODELAY, &v)
        .map_err(io::to_io_error)
}

pub fn join_multicast_group(fd: Fd, addr: &IpAddr, interface: &Option<IpAddr>) -> Result<()> {
    let grp_req = try!(make_ip_mreq(addr, interface));

    nix::setsockopt(fd, nix::IPPROTO_IP, nix::IP_ADD_MEMBERSHIP, &grp_req)
        .map_err(io::to_io_error)
}

pub fn leave_multicast_group(fd: Fd, addr: &IpAddr, interface: &Option<IpAddr>) -> Result<()> {
    let grp_req = try!(make_ip_mreq(addr, interface));

    nix::setsockopt(fd, nix::IPPROTO_IP, nix::IP_ADD_MEMBERSHIP, &grp_req)
        .map_err(io::to_io_error)
}

pub fn set_multicast_ttl(fd: Fd, val: u8) -> Result<()> {
    let v: nix::IpMulticastTtl = val;

    nix::setsockopt(fd, nix::IPPROTO_IP, nix::IP_MULTICAST_TTL, &v)
        .map_err(io::to_io_error)
}

pub fn linger(fd: Fd) -> Result<usize> {
    let mut linger: nix::linger = unsafe { mem::uninitialized() };

    try!(nix::getsockopt(fd, nix::SOL_SOCKET, nix::SO_LINGER, &mut linger)
            .map_err(io::to_io_error));

    if linger.l_onoff > 0 {
        Ok(linger.l_linger as usize)
    } else {
        Ok(0)
    }
}

pub fn getpeername(fd: Fd) -> Result<SocketAddr> {
    let sa : nix::sockaddr_in = unsafe { mem::zeroed() };
    let mut a = nix::SockAddr::SockIpV4(sa);

    try!(nix::getpeername(fd, &mut a).map_err(io::to_io_error));

    Ok(to_sockaddr(&a))
}

pub fn getsockname(fd: Fd) -> Result<SocketAddr> {
    let sa : nix::sockaddr_in = unsafe { mem::zeroed() };
    let mut a = nix::SockAddr::SockIpV4(sa);

    try!(nix::getsockname(fd, &mut a).map_err(io::to_io_error));

    Ok(to_sockaddr(&a))
}

pub fn set_linger(fd: Fd, dur_s: usize) -> Result<()> {
    let linger = nix::linger {
        l_onoff: (if dur_s > 0 { 1 } else { 0 }) as nix::c_int,
        l_linger: dur_s as nix::c_int
    };

    nix::setsockopt(fd, nix::SOL_SOCKET, nix::SO_LINGER, &linger)
        .map_err(io::to_io_error)
}

fn make_ip_mreq(group_addr: &IpAddr, iface_addr: &Option<IpAddr>) -> Result<nix::ip_mreq> {
    Ok(nix::ip_mreq {
        imr_multiaddr: from_ip_addr_to_inaddr(&Some(*group_addr)),
        imr_interface: from_ip_addr_to_inaddr(iface_addr)
    })
}

fn from_ip_addr_to_inaddr(addr: &Option<IpAddr>) -> nix::in_addr {
    match *addr {
        Some(IpAddr::V4(ip)) => ipv4_to_inaddr(ip),
        Some(IpAddr::V6(_)) => unimplemented!(),
        None => nix::in_addr { s_addr: nix::INADDR_ANY },
    }
}

fn to_sockaddr(addr: &nix::SockAddr) -> SocketAddr {
    match *addr {
        nix::SockAddr::SockIpV4(sin) => {
            InetAddr(u32be_to_ipv4(sin.sin_addr.s_addr), Int::from_be(sin.sin_port))
        }
        nix::SockAddr::SockUnix(addr) => {
            let mut str_path = String::new();
            for c in addr.sun_path.iter() {
                if *c == 0 { break; }
                str_path.push(*c as u8 as char);
            }

            UnixAddr(Path::new(str_path))
        }
        _ => unimplemented!()
    }
}

fn from_sockaddr(addr: &SocketAddr) -> nix::SockAddr {
    use std::mem;

    match *addr {
        InetAddr(ip, port) => {
            match ip {
                IPv4Addr(a, b, c, d) => {
                    let mut addr: nix::sockaddr_in = unsafe { mem::zeroed() };

                    addr.sin_family = nix::AF_INET as nix::sa_family_t;
                    addr.sin_port = port.to_be();
                    addr.sin_addr = ipv4_to_inaddr(a, b, c, d);

                    nix::SockAddr::SockIpV4(addr)
                }
                _ => unimplemented!()
            }
        }
        UnixAddr(ref path) => {
            let mut addr: nix::sockaddr_un = unsafe { mem::zeroed() };

            addr.sun_family = nix::AF_UNIX as nix::sa_family_t;

            let c_path_ptr = path.as_vec();
            assert!(c_path_ptr.len() < addr.sun_path.len());
            for (sp_iter, path_iter) in addr.sun_path.iter_mut().zip(c_path_ptr.iter()) {
                *sp_iter = *path_iter as i8;
            }

            nix::SockAddr::SockUnix(addr)
        }
    }
}

fn ipv4_to_u32(a: u8, b: u8, c: u8, d: u8) -> nix::InAddrT {
    Int::from_be(((a as u32) << 24) |
                 ((b as u32) << 16) |
                 ((c as u32) <<  8) |
                 ((d as u32) <<  0))
}

fn u32be_to_ipv4(net: u32) -> IpAddr {
    u32_to_ipv4(Int::from_be(net))
}

fn u32_to_ipv4(net: u32) -> IpAddr {
    IPv4Addr(((net >> 24) & 0xff) as u8,
         ((net >> 16) & 0xff) as u8,
         ((net >> 8) & 0xff) as u8,
         (net & 0xff) as u8)
}

fn ipv4_to_inaddr(a: u8, b: u8, c: u8, d: u8) -> nix::in_addr {
    nix::in_addr {
        s_addr: ipv4_to_u32(a, b, c, d)
    }
}
