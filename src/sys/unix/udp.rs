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

pub(crate) fn recv_with_ancillary_data(
    socket: &net::UdpSocket,
    buf: &mut [u8],
    ancillary_data: &mut [u8],
) -> io::Result<usize> {
    let mut buf = libc::iovec {
        iov_base: buf.as_mut_ptr().cast(),
        iov_len: buf.len(),
    };
    let mut msg_hdr = libc::msghdr {
        msg_iov: &mut buf,
        msg_iovlen: 1,
        msg_name: std::ptr::null_mut(),
        msg_namelen: 0,
        msg_control: ancillary_data.as_mut_ptr().cast(),
        msg_controllen: ancillary_data.len(),
        msg_flags: 0,
    };
    syscall!(recvmsg(socket.as_raw_fd(), &mut msg_hdr, 0)).map(|n| n as usize)
}

