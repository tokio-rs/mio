use super::{socket_addr, SocketAddr};
use crate::sys::unix::net::new_socket;

use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::os::unix::net;
use std::path::Path;

pub(crate) fn bind(path: &Path) -> io::Result<net::UnixDatagram> {
    new_socket(libc::AF_UNIX, libc::SOCK_DGRAM).and_then(|fd| {
        // Ensure the fd is closed.
        let socket = unsafe { net::UnixDatagram::from_raw_fd(fd) };
        socket_addr(path).and_then(|(sockaddr, socklen)| {
            let sockaddr = &sockaddr as *const libc::sockaddr_un as *const _;
            syscall!(bind(fd, sockaddr, socklen)).map(|_| socket)
        })
    })
}

pub(crate) fn unbound() -> io::Result<net::UnixDatagram> {
    new_socket(libc::AF_UNIX, libc::SOCK_DGRAM)
        .map(|socket| unsafe { net::UnixDatagram::from_raw_fd(socket) })
}

pub(crate) fn pair() -> io::Result<(net::UnixDatagram, net::UnixDatagram)> {
    let mut fds = [-1; 2];
    let flags = libc::SOCK_DGRAM;
    #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "solaris")))]
    let flags = flags | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

    syscall!(socketpair(libc::AF_UNIX, flags, 0, fds.as_mut_ptr()))?;
    let pair = unsafe {
        (
            net::UnixDatagram::from_raw_fd(fds[0]),
            net::UnixDatagram::from_raw_fd(fds[1]),
        )
    };

    // Darwin and Solaris do not have `SOCK_NONBLOCK` or `SOCK_CLOEXEC`.
    //
    // In order to set those flags, additional `fcntl` sys calls must be
    // performed. If a `fnctl` fails after the sockets have been created, the
    // file descriptors will leak. Creating `pair` above ensures that if there
    // is an error, the file descriptors are closed.
    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
    {
        syscall!(fcntl(fds[0], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC))?;
        syscall!(fcntl(fds[1], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC))?;
    }
    Ok(pair)
}

// The following functions can't simply be replaced with a call to
// `net::UnixDatagram` because of our `SocketAddr` type.

pub(crate) fn local_addr(socket: &net::UnixDatagram) -> io::Result<SocketAddr> {
    SocketAddr::new(|sockaddr, socklen| {
        syscall!(getsockname(socket.as_raw_fd(), sockaddr, socklen))
    })
}

pub(crate) fn peer_addr(socket: &net::UnixDatagram) -> io::Result<SocketAddr> {
    SocketAddr::new(|sockaddr, socklen| {
        syscall!(getpeername(socket.as_raw_fd(), sockaddr, socklen))
    })
}

pub(crate) fn recv_from(
    socket: &net::UnixDatagram,
    dst: &mut [u8],
) -> io::Result<(usize, SocketAddr)> {
    let mut count = 0;
    let socketaddr = SocketAddr::new(|sockaddr, socklen| {
        syscall!(recvfrom(
            socket.as_raw_fd(),
            dst.as_mut_ptr() as *mut _,
            dst.len(),
            0,
            sockaddr,
            socklen,
        ))
        .map(|c| {
            count = c;
            c as libc::c_int
        })
    })?;
    Ok((count as usize, socketaddr))
}
