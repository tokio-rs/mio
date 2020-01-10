use std::mem::size_of_val;
use std::net::SocketAddr;
#[cfg(feature = "tcp")]
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::sync::Once;

use winapi::ctypes::c_int;
use winapi::shared::ws2def::SOCKADDR;

/// Initialise the network stack for Windows.
pub(crate) fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Let standard library call `WSAStartup` for us, we can't do it
        // ourselves because otherwise using any type in `std::net` would
        // panic when it tries to call `WSAStartup` a second time.
        drop(std::net::UdpSocket::bind("127.0.0.1:0"));
    });
}

pub(crate) fn from_socket_addr(addr: &SocketAddr) -> (*const SOCKADDR, c_int) {
    match addr {
        SocketAddr::V4(ref addr) => (
            addr as *const _ as *const SOCKADDR,
            size_of_val(addr) as c_int,
        ),
        SocketAddr::V6(ref addr) => (
            addr as *const _ as *const SOCKADDR,
            size_of_val(addr) as c_int,
        ),
    }
}

#[cfg(feature = "tcp")]
pub(crate) fn any_socket_addr(other: SocketAddr) -> SocketAddr {
    match other {
        SocketAddr::V4(..) => {
            let any = Ipv4Addr::new(0, 0, 0, 0);
            let addr = SocketAddrV4::new(any, 0);
            SocketAddr::V4(addr)
        }
        SocketAddr::V6(..) => {
            let any = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0);
            let addr = SocketAddrV6::new(any, 0, 0, 0);
            SocketAddr::V6(addr)
        }
    }
}
