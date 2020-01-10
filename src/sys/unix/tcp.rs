use crate::sys::Socket;

use std::io;
use std::net::{self, SocketAddr};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};

pub fn connect(addr: SocketAddr) -> io::Result<net::TcpStream> {
    let socket = Socket::from_addr(addr, libc::SOCK_STREAM, 0)?;

    // Set SO_REUSEADDR (mirrors what libstd does).
    socket.set_reuse_address()?;

    socket.connect(addr)?;
    unsafe { Ok(net::TcpStream::from_raw_fd(socket.into_raw_fd())) }
}

pub fn bind(addr: SocketAddr) -> io::Result<net::TcpListener> {
    let socket = Socket::from_addr(addr, libc::SOCK_STREAM, 0)?;
    socket.bind(addr)?;
    socket.listen(1024)?;
    unsafe { Ok(net::TcpListener::from_raw_fd(socket.into_raw_fd())) }
}

pub fn accept(listener: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    let socket = unsafe { Socket::from_raw_fd(listener.as_raw_fd()) };
    socket.accept().map(|(socket, addr)| {
        let stream = unsafe { net::TcpStream::from_raw_fd(socket.into_raw_fd()) };
        (stream, addr)
    })
}
