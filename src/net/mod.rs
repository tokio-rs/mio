//! Networking primitives
//!
use {MioResult, MioError};
use io::{Io, NonBlock};
use buf::{Buf, MutBuf};
use std::net::SocketAddr;
use std::os::unix::{Fd, AsRawFd};

pub mod tcp;
pub mod udp;
pub mod unix;

pub trait TrySend {
    fn send_to<B: Buf>(&self, buf: &mut B, target: &SocketAddr) -> MioResult<NonBlock<()>>;
}

pub trait TryRecv {
    fn recv_from<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<SocketAddr>>;
}

pub trait TryAccept {
    type Sock;

    fn try_accept(&self) -> MioResult<NonBlock<Self::Sock>>;
}

/*
 *
 * ===== Socket options =====
 *
 */

pub trait Socket : AsRawFd {
    fn linger(&self) -> MioResult<usize> {
        let linger = try!(nix::getsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::Linger)
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

        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::Linger, &linger)
            .map_err(MioError::from_nix_error)
    }

    fn set_reuseaddr(&self, val: bool) -> MioResult<()> {
        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::ReuseAddr, val)
            .map_err(MioError::from_nix_error)
    }

    fn set_reuseport(&self, val: bool) -> MioResult<()> {
        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::ReusePort, val)
            .map_err(MioError::from_nix_error)
    }

    fn set_tcp_nodelay(&self, val: bool) -> MioResult<()> {
        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Tcp, nix::sockopt::TcpNoDelay, val)
            .map_err(MioError::from_nix_error)
    }
}

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

fn socket(family: nix::AddressFamily, ty: nix::SockType) -> MioResult<Fd> {
    nix::socket(family, ty, nix::SOCK_NONBLOCK | nix::SOCK_CLOEXEC)
        .map_err(MioError::from_nix_error)
}

fn connect(io: &Io, addr: &nix::SockAddr) -> MioResult<bool> {
    match nix::connect(io.as_raw_fd(), addr) {
        Ok(_) => Ok(true),
        Err(e) => {
            match e {
                nix::NixError::Sys(nix::EINPROGRESS) => Ok(false),
                _ => Err(MioError::from_nix_error(e))
            }
        }
    }
}

fn bind(io: &Io, addr: &nix::SockAddr) -> MioResult<()> {
    nix::bind(io.as_raw_fd(), addr)
        .map_err(MioError::from_nix_error)
}

fn listen(io: &Io, backlog: usize) -> MioResult<()> {
    nix::listen(io.as_raw_fd(), backlog)
        .map_err(MioError::from_nix_error)
}

fn accept(io: &Io) -> MioResult<Fd> {
    nix::accept4(io.as_raw_fd(), nix::SOCK_NONBLOCK | nix::SOCK_CLOEXEC)
        .map_err(MioError::from_nix_error)
}

// UDP & UDS
#[inline]
fn recvfrom(io: &Io, buf: &mut [u8]) -> MioResult<(usize, nix::SockAddr)> {
    nix::recvfrom(io.as_raw_fd(), buf)
        .map_err(MioError::from_nix_error)
}

// UDP & UDS
#[inline]
fn sendto(io: &Io, buf: &[u8], target: &nix::SockAddr) -> MioResult<usize> {
    nix::sendto(io.as_raw_fd(), buf, target, nix::MSG_DONTWAIT)
        .map_err(MioError::from_nix_error)
}

fn getpeername(io: &Io) -> MioResult<nix::SockAddr> {
    nix::getpeername(io.as_raw_fd())
        .map_err(MioError::from_nix_error)
}

fn getsockname(io: &Io) -> MioResult<nix::SockAddr> {
    nix::getsockname(io.as_raw_fd())
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
