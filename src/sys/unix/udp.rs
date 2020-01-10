use crate::sys::Socket;

use std::io;
use std::net::{self, SocketAddr};
use std::os::unix::io::{FromRawFd, IntoRawFd};

pub fn bind(addr: SocketAddr) -> io::Result<net::UdpSocket> {
    let socket = Socket::from_addr(addr, libc::SOCK_DGRAM, 0)?;

    // Set SO_NOSIGPIPE on iOS and macOS (mirrors what libstd does).
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    socket.set_no_sigpipe()?;

    socket.bind(addr)?;
    unsafe { Ok(net::UdpSocket::from_raw_fd(socket.into_raw_fd())) }
}
