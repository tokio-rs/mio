use error::MioResult;
use sock::{Socket, SockAddr, InetAddr, IpV4Addr};
use reactor::{IoHandle};

mod nix {
    pub use nix::errno::EINPROGRESS;
    pub use nix::sys::socket::*;
    pub use nix::unistd::*;
}

pub fn connect(io: IoHandle, addr: &SockAddr) -> MioResult<bool> {
    match nix::connect(io.ident(), &from_sockaddr(addr)) {
        Ok(_) => Ok(true),
        Err(e) => {
            match e.kind {
                nix::EINPROGRESS => Ok(false),
                _ => Err(e)
            }
        }
    }
}

#[inline]
pub fn read(io: &IoHandle, dst: &mut [u8]) -> MioResult<uint> {
    nix::read(io.ident(), dst)
}

#[inline]
pub fn write(io: &IoHandle, src: &[u8]) -> MioResult<uint> {
    nix::write(io.ident(), src)
}

fn from_sockaddr(addr: &SockAddr) -> nix::SockAddr {
    use std::mem;

    println!("addr: {}", addr);

    match *addr {
        InetAddr(ip, port) => {
            match ip {
                IpV4Addr(a, b, c, d) => {
                    let mut addr: nix::sockaddr_in = unsafe { mem::zeroed() };

                    addr.sin_family = nix::AF_INET as nix::sa_family_t;
                    addr.sin_port = port.to_be();
                    addr.sin_addr = ip4_to_inaddr(a, b, c, d);

                    nix::SockIpV4(addr)
                }
                _ => unimplemented!()
            }
        }
        _ => unimplemented!()
    }
}

fn ip4_to_inaddr(a: u8, b: u8, c: u8, d: u8) -> nix::in_addr {
    let ip = (a as u32 << 24) |
             (b as u32 << 16) |
             (c as u32 <<  8) |
             (d as u32 <<  0);

    nix::in_addr {
        s_addr: Int::from_be(ip)
    }
}
