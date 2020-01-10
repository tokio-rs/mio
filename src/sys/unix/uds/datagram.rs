use super::{socket_addr, SocketAddr};
use crate::sys::Socket;

use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::net;
use std::path::Path;

pub(crate) fn bind(path: &Path) -> io::Result<net::UnixDatagram> {
    let socket = Socket::new(libc::AF_UNIX, libc::SOCK_DGRAM, 0)?;

    // `Socket::bind` does not satisfy this case because of Mio's `SocketAddr`.
    let (storage, len) = socket_addr(path)?;
    let sockaddr = &storage as *const libc::sockaddr_un as *const _;
    match syscall!(bind(socket.as_raw_fd(), sockaddr, len)) {
        Ok(_) => Ok(unsafe { net::UnixDatagram::from_raw_fd(socket.into_raw_fd()) }),
        Err(err) => {
            // Close the socket if we hit an error, ignoring the error
            // from closing since we can't pass back two errors.
            let _ = unsafe { libc::close(socket.into_raw_fd()) };
            Err(err)
        }
    }
}

pub(crate) fn unbound() -> io::Result<net::UnixDatagram> {
    Socket::new(libc::AF_UNIX, libc::SOCK_DGRAM, 0)
        .map(|socket| unsafe { net::UnixDatagram::from_raw_fd(socket.as_raw_fd()) })
}

pub(crate) fn pair() -> io::Result<(net::UnixDatagram, net::UnixDatagram)> {
    super::pair(libc::SOCK_DGRAM)
}

pub(crate) fn local_addr(socket: &net::UnixDatagram) -> io::Result<SocketAddr> {
    super::local_addr(socket.as_raw_fd())
}

pub(crate) fn peer_addr(socket: &net::UnixDatagram) -> io::Result<SocketAddr> {
    super::peer_addr(socket.as_raw_fd())
}

pub(crate) fn recv_from(
    socket: &net::UnixDatagram,
    dst: &mut [u8],
) -> io::Result<(usize, SocketAddr)> {
    let mut count = 0;
    let socketaddr = SocketAddr::new(|sockaddr, socklen| {
        syscall!(recvfrom(
            socket.as_raw_fd(),
            dst.as_mut_ptr() as *mut _,
            dst.len(),
            0,
            sockaddr,
            socklen,
        ))
        .map(|c| {
            count = c;
            c as libc::c_int
        })
    })?;
    Ok((count as usize, socketaddr))
}
