use super::{from_socket_addr, SocketAddr};
use crate::sys::Socket;

use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::net;
use std::path::Path;

pub(crate) fn connect(path: &Path) -> io::Result<net::UnixStream> {
    let socket = Socket::new(libc::AF_UNIX, libc::SOCK_STREAM, 0)?;
    let (storage, len) = from_socket_addr(path)?;
    socket.connect2(
        &storage as *const libc::sockaddr_un as *const libc::sockaddr,
        len,
    )?;
    Ok(unsafe { net::UnixStream::from_raw_fd(socket.into_raw_fd()) })
}

pub(crate) fn pair() -> io::Result<(net::UnixStream, net::UnixStream)> {
    super::pair(libc::SOCK_STREAM)
}

pub(crate) fn local_addr(socket: &net::UnixStream) -> io::Result<SocketAddr> {
    super::local_addr(socket.as_raw_fd())
}

pub(crate) fn peer_addr(socket: &net::UnixStream) -> io::Result<SocketAddr> {
    super::peer_addr(socket.as_raw_fd())
}
