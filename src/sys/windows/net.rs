use std::io;
use std::mem::size_of_val;
use std::net::SocketAddr;
use std::sync::Once;

use winapi::ctypes::c_int;
use winapi::shared::ws2def::SOCKADDR;
use winapi::um::winsock2::{
    ioctlsocket, socket, FIONBIO, INVALID_SOCKET, SOCKET,
};

/// Initialise the network stack for Windows.
pub(crate) fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Let standard library call `WSAStartup` for us, we can't do it
        // ourselves because otherwise using any type in `std::net` would panic
        // when it tries to call `WSAStartup` a second time.
        drop(std::net::UdpSocket::bind("127.0.0.1:0"));
    });
}

/// Create a new non-blocking socket.
#[cfg(feature = "udp")]
pub(crate) fn new_ip_socket(addr: SocketAddr, socket_type: c_int) -> io::Result<SOCKET> {
    use winapi::um::winsock2::{PF_INET, PF_INET6};

    let domain = match addr {
        SocketAddr::V4(..) => PF_INET,
        SocketAddr::V6(..) => PF_INET6,
    };

    new_socket(domain, socket_type)
}

pub(crate) fn new_socket(domain: c_int, socket_type: c_int) -> io::Result<SOCKET> {
    syscall!(
        socket(domain, socket_type, 0),
        PartialEq::eq,
        INVALID_SOCKET
    )
    .and_then(|socket| {
        syscall!(ioctlsocket(socket, FIONBIO, &mut 1), PartialEq::ne, 0).map(|_| socket as SOCKET)
    })
}

pub(crate) fn socket_addr(addr: &SocketAddr) -> (*const SOCKADDR, c_int) {
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
