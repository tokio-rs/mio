use std::io;
use std::net::{self, SocketAddr};

pub fn connect(_: SocketAddr) -> io::Result<net::TcpStream> {
    os_required!();
}

pub fn bind(_: SocketAddr) -> io::Result<net::TcpListener> {
    os_required!();
}

pub fn accept(_: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    os_required!();
}
