use std::io;
use std::mem::{size_of, MaybeUninit};
use std::net::{self, SocketAddr};
use std::os::unix::io::{AsRawFd, FromRawFd};

use crate::sys::unix::net::{new_ip_socket, socket_addr, to_socket_addr};

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

pub fn bind(addr: SocketAddr) -> io::Result<net::TcpListener> {
    new_ip_socket(addr, libc::SOCK_STREAM).and_then(|socket| {
        // Set SO_REUSEADDR (mirrors what libstd does).
        syscall!(setsockopt(
            socket,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &1 as *const libc::c_int as *const libc::c_void,
            size_of::<libc::c_int>() as libc::socklen_t,
        ))
        .and_then(|_| {
            let (raw_addr, raw_addr_length) = socket_addr(&addr);
            syscall!(bind(socket, raw_addr, raw_addr_length))
        })
        .and_then(|_| syscall!(listen(socket, 1024)))
        .map_err(|err| {
            // Close the socket if we hit an error, ignoring the error
            // from closing since we can't pass back two errors.
            let _ = unsafe { libc::close(socket) };
            err
        })
        .map(|_| unsafe { net::TcpListener::from_raw_fd(socket) })
    })
}

pub fn accept(listener: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    let mut addr: MaybeUninit<libc::sockaddr_storage> = MaybeUninit::uninit();
    let mut length = size_of::<libc::sockaddr_storage>() as libc::socklen_t;

    // On platforms that support it we can use `accept4(2)` to set `NONBLOCK`
    // and `CLOEXEC` in the call to accept the connection.
    #[cfg(any(
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "linux",
        target_os = "openbsd"
    ))]
    let stream = {
        syscall!(accept4(
            listener.as_raw_fd(),
            addr.as_mut_ptr() as *mut _,
            &mut length,
            libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
        ))
        .map(|socket| unsafe { net::TcpStream::from_raw_fd(socket) })
    }?;

    // But not all platforms have the `accept4(2)` call. Luckily BSD (derived)
    // OSes inherit the non-blocking flag from the listener, so we just have to
    // set `CLOEXEC`.
    #[cfg(any(
        target_os = "ios",
        target_os = "macos",
        // NetBSD 8.0 actually has `accept4(2)`, but libc doesn't expose it
        // (yet). See https://github.com/rust-lang/libc/issues/1636.
        target_os = "netbsd",
        target_os = "solaris",
    ))]
    let stream = {
        syscall!(accept(
            listener.as_raw_fd(),
            addr.as_mut_ptr() as *mut _,
            &mut length
        ))
        .map(|socket| unsafe { net::TcpStream::from_raw_fd(socket) })
        .and_then(|s| syscall!(fcntl(s.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC)).map(|_| s))
    }?;

    // This is safe because `accept` calls above ensures the address
    // initialised.
    unsafe { to_socket_addr(addr.as_ptr()) }.map(|addr| (stream, addr))
}
