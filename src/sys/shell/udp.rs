use std::io;
use std::net::{self, SocketAddr};

pub use std::net::UdpSocket;

pub fn bind(_: SocketAddr) -> io::Result<net::UdpSocket> {
    os_required!()
}

pub(crate) fn only_v6(_: &net::UdpSocket) -> io::Result<bool> {
    os_required!()
}
