use std::io;
use std::net::{self, SocketAddr};
use std::time::Duration;

pub(crate) type TcpSocket = i32;

pub(crate) fn new_v4_socket() -> io::Result<TcpSocket> {
    os_required!();
}

pub(crate) fn new_v6_socket() -> io::Result<TcpSocket> {
    os_required!();
}

pub(crate) fn bind(_socket: TcpSocket, _addr: SocketAddr) -> io::Result<()> {
    os_required!();
}

pub(crate) fn connect(_: TcpSocket, _addr: SocketAddr) -> io::Result<net::TcpStream> {
    os_required!();
}

pub(crate) fn listen(_: TcpSocket, _: u32) -> io::Result<net::TcpListener> {
    os_required!();
}

pub(crate) fn close(_: TcpSocket) {
    os_required!();
}

pub(crate) fn set_reuseaddr(_: TcpSocket, _: bool) -> io::Result<()> {
    os_required!();
}

pub(crate) fn get_reuseaddr(_: TcpSocket) -> io::Result<bool> {
    os_required!();
}

#[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
pub(crate) fn set_reuseport(_: TcpSocket, _: bool) -> io::Result<()> {
    os_required!();
}

#[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
pub(crate) fn get_reuseport(_: TcpSocket) -> io::Result<bool> {
    os_required!();
}

pub(crate) fn set_linger(_: TcpSocket, _: Option<Duration>) -> io::Result<()> {
    os_required!();
}

pub fn accept(_: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    os_required!();
}

pub(crate) fn get_localaddr(_: TcpSocket) -> io::Result<SocketAddr> {
    os_required!();
}
