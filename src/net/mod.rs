//! Networking primitives
//!
use {NonBlock};
use io::{self, Io};
use std::net::SocketAddr;
use std::os::unix::{Fd, AsRawFd};

pub mod tcp;
pub mod udp;
pub mod unix;

/*
 *
 * ===== Socket options =====
 *
 */

pub trait Socket : AsRawFd {

    /// Returns the value for the `SO_LINGER` socket option.
    fn linger(&self) -> io::Result<usize> {
        let linger = try!(nix::getsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::Linger)
            .map_err(io::from_nix_error));

        if linger.l_onoff > 0 {
            Ok(linger.l_onoff as usize)
        } else {
            Ok(0)
        }
    }

    /// Sets the value for the `SO_LINGER` socket option
    fn set_linger(&self, dur_s: usize) -> io::Result<()> {
        let linger = nix::linger {
            l_onoff: (if dur_s > 0 { 1 } else { 0 }) as nix::c_int,
            l_linger: dur_s as nix::c_int
        };

        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::Linger, &linger)
            .map_err(io::from_nix_error)
    }

    fn set_reuseaddr(&self, val: bool) -> io::Result<()> {
        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::ReuseAddr, val)
            .map_err(io::from_nix_error)
    }

    fn set_reuseport(&self, val: bool) -> io::Result<()> {
        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::ReusePort, val)
            .map_err(io::from_nix_error)
    }

    fn set_tcp_nodelay(&self, val: bool) -> io::Result<()> {
        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Tcp, nix::sockopt::TcpNoDelay, val)
            .map_err(io::from_nix_error)
    }

    /// Sets the `SO_RCVTIMEO` socket option to the supplied number of
    /// milliseconds.
    ///
    /// This function is hardcoded to milliseconds until Rust std includes a
    /// stable duration type.
    fn set_read_timeout_ms(&self, val: usize) -> io::Result<()> {
        let t = nix::TimeVal::milliseconds(val as i64);
        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::ReceiveTimeout, &t)
            .map_err(io::from_nix_error)
    }

    /// Sets the `SO_SNDTIMEO` socket option to the supplied number of
    /// milliseconds.
    ///
    /// This function is hardcoded to milliseconds until Rust std includes a
    /// stable duration type.
    fn set_write_timeout_ms(&self, val: usize) -> io::Result<()> {
        let t = nix::TimeVal::milliseconds(val as i64);
        nix::setsockopt(self.as_raw_fd(), nix::SockLevel::Socket, nix::sockopt::SendTimeout, &t)
            .map_err(io::from_nix_error)
    }
}

impl<S: Socket> Socket for NonBlock<S> {
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
    pub use nix::errno::{EINPROGRESS, EAGAIN};
    pub use nix::fcntl::{fcntl, FcntlArg, O_NONBLOCK};
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
    pub use nix::sys::time::TimeVal;
    pub use nix::unistd::{
        read,
        write
    };
}

fn socket(family: nix::AddressFamily, ty: nix::SockType, nonblock: bool) -> io::Result<Fd> {
    let opts = if nonblock {
        nix::SOCK_NONBLOCK | nix::SOCK_CLOEXEC
    } else {
        nix::SOCK_CLOEXEC
    };

    nix::socket(family, ty, opts)
        .map_err(io::from_nix_error)
}

fn connect(io: &Io, addr: &nix::SockAddr) -> io::Result<bool> {
    match nix::connect(io.as_raw_fd(), addr) {
        Ok(_) => Ok(true),
        Err(e) => {
            match e {
                nix::NixError::Sys(nix::EINPROGRESS) => Ok(false),
                _ => Err(io::from_nix_error(e))
            }
        }
    }
}

fn bind(io: &Io, addr: &nix::SockAddr) -> io::Result<()> {
    nix::bind(io.as_raw_fd(), addr)
        .map_err(io::from_nix_error)
}

fn listen(io: &Io, backlog: usize) -> io::Result<()> {
    nix::listen(io.as_raw_fd(), backlog)
        .map_err(io::from_nix_error)
}

fn accept(io: &Io, nonblock: bool) -> io::Result<Fd> {
    let opts = if nonblock {
        nix::SOCK_NONBLOCK | nix::SOCK_CLOEXEC
    } else {
        nix::SOCK_CLOEXEC
    };

    nix::accept4(io.as_raw_fd(), opts)
        .map_err(io::from_nix_error)
}

// UDP & UDS
#[inline]
fn recvfrom(io: &Io, buf: &mut [u8]) -> io::Result<(usize, nix::SockAddr)> {
    nix::recvfrom(io.as_raw_fd(), buf)
        .map_err(io::from_nix_error)
}

// UDP & UDS
#[inline]
fn sendto(io: &Io, buf: &[u8], target: &nix::SockAddr) -> io::Result<usize> {
    nix::sendto(io.as_raw_fd(), buf, target, nix::MSG_DONTWAIT)
        .map_err(io::from_nix_error)
}

fn getpeername(io: &Io) -> io::Result<nix::SockAddr> {
    nix::getpeername(io.as_raw_fd())
        .map_err(io::from_nix_error)
}

fn getsockname(io: &Io) -> io::Result<nix::SockAddr> {
    nix::getsockname(io.as_raw_fd())
        .map_err(io::from_nix_error)
}

fn set_non_block(io: &Io) -> io::Result<()> {
    nix::fcntl(io.as_raw_fd(), nix::FcntlArg::F_SETFL(nix::O_NONBLOCK))
        .map_err(io::from_nix_error)
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
