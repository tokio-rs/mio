use crate::net::{SocketAddr, UnixStream};
use std::io;
use std::os::unix::net;
use std::path::Path;

pub(crate) fn bind(_: &Path) -> io::Result<net::UnixListener> {
    os_required!()
}

pub(crate) fn accept(_: &net::UnixListener) -> io::Result<(UnixStream, SocketAddr)> {
    os_required!()
}

pub(crate) fn local_addr(_: &net::UnixListener) -> io::Result<SocketAddr> {
    os_required!()
}
