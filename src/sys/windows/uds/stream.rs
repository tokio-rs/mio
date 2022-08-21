use crate::sys::windows::std::net;
use std::io;
use std::os::windows::io::AsRawSocket;
use std::path::Path;

pub(crate) fn connect(path: &Path) -> io::Result<net::UnixStream> {
    let socket = net::UnixStream::connect(path)?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

pub(crate) fn pair() -> io::Result<(net::UnixStream, net::UnixStream)> {
    let (stream0, stream1) = net::UnixStream::pair()?;
    stream0.set_nonblocking(true)?;
    stream1.set_nonblocking(true)?;
    Ok((stream0, stream1))
}

pub(crate) fn local_addr(socket: &net::UnixStream) -> io::Result<net::SocketAddr> {
    super::local_addr(socket.as_raw_socket())
}

pub(crate) fn peer_addr(socket: &net::UnixStream) -> io::Result<net::SocketAddr> {
    super::peer_addr(socket.as_raw_socket())
}
