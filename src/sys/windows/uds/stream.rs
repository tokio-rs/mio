use crate::sys::windows::stdnet as net;
use super::SocketAddr;
use std::io;
use std::os::windows::io::AsRawSocket;
use std::path::Path;

pub(crate) fn connect(path: &Path) -> io::Result<net::UnixStream> {
    let socket = net::UnixStream::connect(path)?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

pub(crate) fn local_addr(socket: &net::UnixStream) -> io::Result<SocketAddr> {
    super::local_addr(socket.as_raw_socket())
}

pub(crate) fn peer_addr(socket: &net::UnixStream) -> io::Result<SocketAddr> {
    super::peer_addr(socket.as_raw_socket())
}
