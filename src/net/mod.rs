//! Networking primitives
//!
use io::{IoHandle, NonBlock};
use error::MioResult;
use buf::{Buf, MutBuf};
use os;
use std::net::{SocketAddr, IpAddr};

pub mod tcp;
pub mod udp;
pub mod unix;

mod nix {
    pub use nix::sys::socket::{
        AddressFamily,
        SockAddr,
        SockType,
        InetAddr
    };
}

pub trait Socket : IoHandle {
    fn linger(&self) -> MioResult<usize> {
        os::linger(self.desc())
    }

    fn set_linger(&self, dur_s: usize) -> MioResult<()> {
        os::set_linger(self.desc(), dur_s)
    }

    fn set_reuseaddr(&self, val: bool) -> MioResult<()> {
        os::set_reuseaddr(self.desc(), val)
    }

    fn set_reuseport(&self, val: bool) -> MioResult<()> {
        os::set_reuseport(self.desc(), val)
    }
}

pub trait MulticastSocket : Socket {
    fn join_multicast_group(&self, addr: &IpAddr, interface: Option<&IpAddr>) -> MioResult<()> {
        os::join_multicast_group(self.desc(), addr, interface)
    }

    fn leave_multicast_group(&self, addr: &IpAddr, interface: Option<&IpAddr>) -> MioResult<()> {
        os::leave_multicast_group(self.desc(), addr, interface)
    }

    fn set_multicast_ttl(&self, val: u8) -> MioResult<()> {
        os::set_multicast_ttl(self.desc(), val)
    }
}

// TODO: Break up into TrySend and TryRecv
pub trait UnconnectedSocket {

    fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &SocketAddr) -> MioResult<NonBlock<()>>;

    fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> MioResult<NonBlock<SocketAddr>>;
}

/*
 *
 * ===== Implementation =====
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
