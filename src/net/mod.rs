//! Networking primitives
//!
use {MioResult, MioError};
use io::{IoHandle, NonBlock};
use buf::{Buf, MutBuf};
use std::net::{SocketAddr, IpAddr};

pub mod tcp;
pub mod udp;
pub mod unix;


/*
 *
 * ===== Socket options =====
 *
 */

pub trait Socket : IoHandle {
    fn linger(&self) -> MioResult<usize> {
        let linger = try!(nix::getsockopt(self.desc().fd, nix::SockLevel::Socket, nix::sockopt::Linger)
            .map_err(MioError::from_nix_error));

        if linger.l_onoff > 0 {
            Ok(linger.l_onoff as usize)
        } else {
            Ok(0)
        }
    }

    fn set_linger(&self, dur_s: usize) -> MioResult<()> {
        let linger = nix::linger {
            l_onoff: (if dur_s > 0 { 1 } else { 0 }) as nix::c_int,
            l_linger: dur_s as nix::c_int
        };

        nix::setsockopt(self.desc().fd, nix::SockLevel::Socket, nix::sockopt::Linger, &linger)
            .map_err(MioError::from_nix_error)
    }

    fn set_reuseaddr(&self, val: bool) -> MioResult<()> {
        nix::setsockopt(self.desc().fd, nix::SockLevel::Socket, nix::sockopt::ReuseAddr, val)
            .map_err(MioError::from_nix_error)
    }

    fn set_reuseport(&self, val: bool) -> MioResult<()> {
        nix::setsockopt(self.desc().fd, nix::SockLevel::Socket, nix::sockopt::ReusePort, val)
            .map_err(MioError::from_nix_error)
    }

    fn set_tcp_nodelay(&self, val: bool) -> MioResult<()> {
        nix::setsockopt(self.desc().fd, nix::SockLevel::Tcp, nix::sockopt::TcpNoDelay, val)
            .map_err(MioError::from_nix_error)
    }
}

// TODO: Rename -> Multicast
pub trait MulticastSocket : Socket {
    // TODO: Rename -> join_group
    fn join_multicast_group(&self, addr: &IpAddr, interface: Option<&IpAddr>) -> MioResult<()> {
        match *addr {
            IpAddr::V4(ref addr) => {
                // Ensure interface is the correct family
                let interface = match interface {
                    Some(&IpAddr::V4(ref addr)) => Some(nix::Ipv4Addr::from_std(addr)),
                    Some(_) => return Err(MioError::other()),
                    None => None,
                };

                // Create the request
                let req = nix::ip_mreq::new(nix::Ipv4Addr::from_std(addr), interface);

                // Set the socket option
                nix::setsockopt(self.desc().fd, nix::SockLevel::Ip, nix::sockopt::IpAddMembership, &req)
                    .map_err(MioError::from_nix_error)
            }
            _ => unimplemented!(),
        }
    }

    // TODO: Rename -> leave_group
    fn leave_multicast_group(&self, addr: &IpAddr, interface: Option<&IpAddr>) -> MioResult<()> {
        match *addr {
            IpAddr::V4(ref addr) => {
                // Ensure interface is the correct family
                let interface = match interface {
                    Some(&IpAddr::V4(ref addr)) => Some(nix::Ipv4Addr::from_std(addr)),
                    Some(_) => return Err(MioError::other()),
                    None => None,
                };

                // Create the request
                let req = nix::ip_mreq::new(nix::Ipv4Addr::from_std(addr), interface);

                // Set the socket option
                nix::setsockopt(self.desc().fd, nix::SockLevel::Ip, nix::sockopt::IpDropMembership, &req)
                    .map_err(MioError::from_nix_error)
            }
            _ => unimplemented!(),
        }
    }

    // TODO: Rename -> set_ttl
    fn set_multicast_ttl(&self, val: u8) -> MioResult<()> {
        nix::setsockopt(self.desc().fd, nix::SockLevel::Ip, nix::sockopt::IpMulticastTtl, val)
            .map_err(MioError::from_nix_error)
    }
}

// TODO:
//  - Break up into TrySend and TryRecv.
//  - Return the amount read / writen
pub trait UnconnectedSocket {

    fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &SocketAddr) -> MioResult<NonBlock<()>>;

    fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> MioResult<NonBlock<SocketAddr>>;
}

pub trait BroadcastSocket: Socket {
    fn set_broadcast(&self, val: bool) -> MioResult<()> {
        nix::setsockopt(self.desc().fd, nix::SockLevel::Socket, nix::sockopt::Broadcast, val)
            .map_err(MioError::from_nix_error)
    }
}


/*
 *
 * ===== Implementation =====
 *
 */

use os::IoDesc;

/*
 *
 * ====== Re-exporting needed nix types ======
 *
 */

mod nix {
    pub use nix::{
        c_int,
        NixError,
    };
    pub use nix::errno::EINPROGRESS;
    pub use nix::sys::socket::{
        sockopt,
        AddressFamily,
        SockAddr,
        SockType,
        SockLevel,
        InetAddr,
        Ipv4Addr,
        MSG_DONTWAIT,
        SOCK_NONBLOCK,
        SOCK_CLOEXEC,
        accept4,
        bind,
        connect,
        getpeername,
        getsockname,
        getsockopt,
        ip_mreq,
        linger,
        listen,
        recvfrom,
        sendto,
        setsockopt,
        socket,
    };

    pub use nix::unistd::{
        read,
        write
    };
}

pub fn socket(family: nix::AddressFamily, ty: nix::SockType) -> MioResult<IoDesc> {
    nix::socket(family, ty, nix::SOCK_NONBLOCK | nix::SOCK_CLOEXEC)
        .map(|fd| IoDesc { fd: fd })
        .map_err(MioError::from_nix_error)
}

pub fn connect(io: &IoDesc, addr: &nix::SockAddr) -> MioResult<bool> {
    match nix::connect(io.fd, addr) {
        Ok(_) => Ok(true),
        Err(e) => {
            match e {
                nix::NixError::Sys(nix::EINPROGRESS) => Ok(false),
                _ => Err(MioError::from_nix_error(e))
            }
        }
    }
}

pub fn bind(io: &IoDesc, addr: &nix::SockAddr) -> MioResult<()> {
    nix::bind(io.fd, addr)
        .map_err(MioError::from_nix_error)
}

pub fn listen(io: &IoDesc, backlog: usize) -> MioResult<()> {
    nix::listen(io.fd, backlog)
        .map_err(MioError::from_nix_error)
}

pub fn accept(io: &IoDesc) -> MioResult<IoDesc> {
    Ok(IoDesc {
        fd: try!(nix::accept4(io.fd, nix::SOCK_NONBLOCK | nix::SOCK_CLOEXEC)
                     .map_err(MioError::from_nix_error))
    })
}

// UDP & UDS
#[inline]
pub fn recvfrom(io: &IoDesc, buf: &mut [u8]) -> MioResult<(usize, nix::SockAddr)> {
    nix::recvfrom(io.fd, buf).map_err(MioError::from_nix_error)
}

// UDP & UDS
#[inline]
pub fn sendto(io: &IoDesc, buf: &[u8], target: &nix::SockAddr) -> MioResult<usize> {
    nix::sendto(io.fd, buf, target, nix::MSG_DONTWAIT)
        .map_err(MioError::from_nix_error)
}

/*
 *
 * ===== Read / Write =====
 *
 */

#[inline]
pub fn read(io: &IoDesc, dst: &mut [u8]) -> MioResult<usize> {
    let res = try!(nix::read(io.fd, dst).map_err(MioError::from_nix_error));

    if res == 0 {
        return Err(MioError::eof());
    }

    Ok(res)
}

#[inline]
pub fn write(io: &IoDesc, src: &[u8]) -> MioResult<usize> {
    nix::write(io.fd, src).map_err(MioError::from_nix_error)
}

pub fn getpeername(io: &IoDesc) -> MioResult<nix::SockAddr> {
    nix::getpeername(io.fd)
        .map_err(MioError::from_nix_error)
}

pub fn getsockname(io: &IoDesc) -> MioResult<nix::SockAddr> {
    nix::getsockname(io.fd)
        .map_err(MioError::from_nix_error)
}

/*
 *
 * ===== Helpers =====
 *
 */

fn to_nix_addr(addr: &SocketAddr) -> nix::SockAddr {
    nix::SockAddr::Inet(nix::InetAddr::from_std(addr))
}

fn to_std_addr(addr: nix::SockAddr) -> SocketAddr {
    match addr {
        nix::SockAddr::Inet(ref addr) => addr.to_std(),
        _ => panic!("unexpected unix socket address"),
    }
}
