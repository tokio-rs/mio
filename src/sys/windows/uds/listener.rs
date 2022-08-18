use std::io;
use std::os::windows::io::AsRawSocket;
use std::path::Path;

use super::{stdnet as net};
use crate::net::{SocketAddr, UnixStream};

pub(crate) fn bind(path: &Path) -> io::Result<net::UnixListener> {
    let listener = net::UnixListener::bind(path)?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

pub(crate) fn accept(listener: &net::UnixListener) -> io::Result<(UnixStream, SocketAddr)> {
    listener.set_nonblocking(true)?;
    let es = listener.accept().map(|(stream, addr)| (UnixStream::from_std(stream), addr));
    es
}

pub(crate) fn local_addr(listener: &net::UnixListener) -> io::Result<SocketAddr> {
    super::local_addr(listener.as_raw_socket())
}
