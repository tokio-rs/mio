#![cfg(any(feature = "tcp", feature = "udp"))]

use std::net::SocketAddr;

pub(crate) fn from_socket_addr(addr: &SocketAddr) -> (*const libc::sockaddr, libc::socklen_t) {
    use std::mem::size_of_val;

    match addr {
        SocketAddr::V4(ref addr) => (
            addr as *const _ as *const libc::sockaddr,
            size_of_val(addr) as libc::socklen_t,
        ),
        SocketAddr::V6(ref addr) => (
            addr as *const _ as *const libc::sockaddr,
            size_of_val(addr) as libc::socklen_t,
        ),
    }
}

/// `storage` must be initialised to `sockaddr_in` or `sockaddr_in6`.
#[cfg(feature = "tcp")]
pub(crate) unsafe fn to_socket_addr(
    storage: *const libc::sockaddr_storage,
) -> std::io::Result<SocketAddr> {
    match (*storage).ss_family as libc::c_int {
        libc::AF_INET => Ok(SocketAddr::V4(
            *(storage as *const libc::sockaddr_in as *const _),
        )),
        libc::AF_INET6 => Ok(SocketAddr::V6(
            *(storage as *const libc::sockaddr_in6 as *const _),
        )),
        _ => Err(std::io::ErrorKind::InvalidInput.into()),
    }
}
