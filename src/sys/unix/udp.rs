use crate::sys::unix::net::{new_ip_socket, socket_addr};

use std::io;
use std::mem;
use std::net::{self, SocketAddr};
use std::os::unix::io::{AsRawFd, FromRawFd};

pub fn bind(addr: SocketAddr) -> io::Result<net::UdpSocket> {
    let fd = new_ip_socket(addr, libc::SOCK_DGRAM)?;
    let socket = unsafe { net::UdpSocket::from_raw_fd(fd) };

    let (raw_addr, raw_addr_length) = socket_addr(&addr);
    syscall!(bind(fd, raw_addr.as_ptr(), raw_addr_length))?;

    Ok(socket)
}

pub(crate) fn only_v6(socket: &net::UdpSocket) -> io::Result<bool> {
    let mut optval: libc::c_int = 0;
    let mut optlen = mem::size_of::<libc::c_int>() as libc::socklen_t;

    syscall!(getsockopt(
        socket.as_raw_fd(),
        libc::IPPROTO_IPV6,
        libc::IPV6_V6ONLY,
        &mut optval as *mut _ as *mut _,
        &mut optlen,
    ))?;

    Ok(optval != 0)
}
