use std::io;
use std::net::{self, SocketAddr};
use std::os::unix::io::FromRawFd;

use crate::sys::unix::net::{new_ip_socket, socket_addr};

mod listener;
pub(crate) use self::listener::TcpListener;

pub fn connect(addr: SocketAddr) -> io::Result<net::TcpStream> {
    new_ip_socket(addr, libc::SOCK_STREAM)
        .and_then(|socket| {
            let (raw_addr, raw_addr_length) = socket_addr(&addr);
            syscall!(connect(socket, raw_addr, raw_addr_length))
                .or_else(|err| match err {
                    // Connect hasn't finished, but that is fine.
                    ref err if err.raw_os_error() == Some(libc::EINPROGRESS) => Ok(0),
                    err => Err(err),
                })
                .map(|_| socket)
                .map_err(|err| {
                    // Close the socket if we hit an error, ignoring the error
                    // from closing since we can't pass back two errors.
                    let _ = unsafe { libc::close(socket) };
                    err
                })
        })
        .map(|socket| unsafe { net::TcpStream::from_raw_fd(socket) })
}
