use std::mem;
use error::{MioResult, MioError};
use io::IoHandle;
use net::{AddressFamily, SockAddr, IpAddr, SockType};
use nix::sys::socket::{sockopt, SockLevel};

mod nix {
    pub use nix::{c_int, NixError};
    pub use nix::fcntl::{Fd, O_NONBLOCK, O_CLOEXEC};
    pub use nix::errno::EINPROGRESS;
    pub use nix::sys::socket::*;
    pub use nix::unistd::*;
}

/*
 *
 * ===== Awakener =====
 *
 */

pub struct PipeAwakener {
    reader: IoDesc,
    writer: IoDesc
}

impl PipeAwakener {
    pub fn new() -> MioResult<PipeAwakener> {
        let (rd, wr) = try!(pipe());

        Ok(PipeAwakener {
            reader: rd,
            writer: wr
        })
    }

    pub fn wakeup(&self) -> MioResult<()> {
        write(&self.writer, b"0x01")
            .map(|_| ())
    }

    pub fn desc(&self) -> &IoDesc {
        &self.reader
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

/// Represents the OS's handle to the IO instance. In this case, it is the file
/// descriptor.
#[derive(Debug)]
pub struct IoDesc {
    pub fd: nix::Fd
}

impl IoHandle for IoDesc {
    fn desc(&self) -> &IoDesc {
        self
    }
}

impl Drop for IoDesc {
    fn drop(&mut self) {
        let _ = nix::close(self.fd);
    }
}

/*
 *
 * ===== Pipes =====
 *
 */

pub fn pipe() -> MioResult<(IoDesc, IoDesc)> {
    let (rd, wr) = try!(nix::pipe2(nix::O_NONBLOCK | nix::O_CLOEXEC)
                        .map_err(MioError::from_nix_error));

    Ok((IoDesc { fd: rd }, IoDesc { fd: wr }))
}

/*
 *
 * ===== Sockets =====
 *
 */

pub fn socket(family: AddressFamily, ty: SockType) -> MioResult<IoDesc> {
    nix::socket(family, ty, nix::SOCK_NONBLOCK | nix::SOCK_CLOEXEC)
        .map(|fd| IoDesc { fd: fd })
        .map_err(MioError::from_nix_error)
}

pub fn connect(io: &IoDesc, addr: &SockAddr) -> MioResult<bool> {
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

pub fn bind(io: &IoDesc, addr: &SockAddr) -> MioResult<()> {
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

#[inline]
pub fn recvfrom(io: &IoDesc, buf: &mut [u8]) -> MioResult<(usize, SockAddr)> {
    match nix::recvfrom(io.fd, buf).map_err(MioError::from_nix_error) {
        Ok((cnt, ref addr)) => {
            let addr = nix::FromSockAddr::from_sock_addr(addr)
                .expect("not currently supported");

            Ok((cnt, addr))
        },
        Err(e) => Err(e)
    }
}

#[inline]
pub fn sendto(io: &IoDesc, buf: &[u8], target: &SockAddr) -> MioResult<usize> {
    nix::sendto(io.fd, buf, target, nix::MSG_DONTWAIT)
        .map_err(MioError::from_nix_error)
}

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

// ===== Socket options =====

pub fn reuseaddr(_io: &IoDesc) -> MioResult<usize> {
    unimplemented!()
}

pub fn set_reuseaddr(io: &IoDesc, val: bool) -> MioResult<()> {
    nix::setsockopt(io.fd, SockLevel::Socket, sockopt::ReuseAddr, val)
        .map_err(MioError::from_nix_error)
}

pub fn set_reuseport(io: &IoDesc, val: bool) -> MioResult<()> {
    nix::setsockopt(io.fd, SockLevel::Socket, sockopt::ReusePort, val)
        .map_err(MioError::from_nix_error)
}

pub fn set_tcp_nodelay(io: &IoDesc, val: bool) -> MioResult<()> {
    nix::setsockopt(io.fd, SockLevel::Tcp, sockopt::TcpNoDelay, val)
        .map_err(MioError::from_nix_error)
}

pub fn join_multicast_group(io: &IoDesc, addr: &IpAddr, interface: Option<&IpAddr>) -> MioResult<()> {
    let req = try!(nix::ip_mreq::new(addr, interface).map_err(MioError::from_nix_error));

    nix::setsockopt(io.fd, SockLevel::Ip, sockopt::IpAddMembership, &req)
        .map_err(MioError::from_nix_error)
}

pub fn leave_multicast_group(io: &IoDesc, addr: &IpAddr, interface: Option<&IpAddr>) -> MioResult<()> {
    let req = try!(nix::ip_mreq::new(addr, interface).map_err(MioError::from_nix_error));

    nix::setsockopt(io.fd, SockLevel::Ip, sockopt::IpDropMembership, &req)
        .map_err(MioError::from_nix_error)
}

pub fn set_multicast_ttl(io: &IoDesc, val: u8) -> MioResult<()> {
    nix::setsockopt(io.fd, SockLevel::Ip, sockopt::IpMulticastTtl, val)
        .map_err(MioError::from_nix_error)
}

pub fn linger(io: &IoDesc) -> MioResult<usize> {
    let linger = try!(nix::getsockopt(io.fd, SockLevel::Socket, sockopt::Linger)
        .map_err(MioError::from_nix_error));

    if linger.l_onoff > 0 {
        Ok(linger.l_onoff as usize)
    } else {
        Ok(0)
    }
}

pub fn set_linger(io: &IoDesc, dur_s: usize) -> MioResult<()> {
    let linger = nix::linger {
        l_onoff: (if dur_s > 0 { 1 } else { 0 }) as nix::c_int,
        l_linger: dur_s as nix::c_int
    };

    nix::setsockopt(io.fd, SockLevel::Socket, sockopt::Linger, &linger)
        .map_err(MioError::from_nix_error)
}

pub fn getpeername(io: &IoDesc) -> MioResult<SockAddr> {
    nix::getpeername(io.fd)
        .map(|addr| nix::FromSockAddr::from_sock_addr(&addr).expect("expected a value"))
        .map_err(MioError::from_nix_error)
}

pub fn getsockname(io: &IoDesc) -> MioResult<SockAddr> {
    nix::getsockname(io.fd)
        .map(|addr| nix::FromSockAddr::from_sock_addr(&addr).expect("expected a value"))
        .map_err(MioError::from_nix_error)
}
