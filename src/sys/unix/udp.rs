use crate::sys::unix::net::{new_ip_socket, socket_addr};

use std::io;
use std::net::{self, SocketAddr};
use std::os::unix::io::FromRawFd;

pub fn bind(addr: SocketAddr) -> io::Result<net::UdpSocket> {
    // Gives a warning for non Apple platforms.
    #[allow(clippy::let_and_return)]
    let socket = new_ip_socket(addr, libc::SOCK_DGRAM);

    // Set SO_NOSIGPIPE on iOS and macOS (mirrors what libstd does).
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    let socket = socket.and_then(|socket| {
        syscall!(setsockopt(
            socket,
            libc::SOL_SOCKET,
            libc::SO_NOSIGPIPE,
            &1 as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        ))
        .map(|_| socket)
    });

    socket.and_then(|socket| {
        let (raw_addr, raw_addr_length) = socket_addr(&addr);
        syscall!(bind(socket, raw_addr, raw_addr_length))
            .map_err(|err| {
                // Close the socket if we hit an error, ignoring the error
                // from closing since we can't pass back two errors.
                let _ = unsafe { libc::close(socket) };
                err
            })
            .map(|_| unsafe { net::UdpSocket::from_raw_fd(socket) })
    })
}
