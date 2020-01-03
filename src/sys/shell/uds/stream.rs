use std::io;
use std::os::unix::net;
use std::path::Path;

use crate::net::SocketAddr;

pub(crate) fn connect(_: &Path) -> io::Result<net::UnixStream> {
    os_required!()
}

pub(crate) fn pair() -> io::Result<(net::UnixStream, net::UnixStream)> {
    os_required!()
}

pub(crate) fn local_addr(_: &net::UnixStream) -> io::Result<SocketAddr> {
    os_required!()
}

pub(crate) fn peer_addr(_: &net::UnixStream) -> io::Result<SocketAddr> {
    os_required!()
}
