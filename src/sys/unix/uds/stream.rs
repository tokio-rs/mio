use super::{socket_addr, SocketAddr};
use crate::sys::unix::net::new_socket;

use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::os::unix::net;
use std::path::Path;

pub(crate) fn connect(path: &Path) -> io::Result<net::UnixStream> {
    let socket = new_socket(libc::AF_UNIX, libc::SOCK_STREAM)?;
    let (sockaddr, socklen) = socket_addr(path)?;
    let sockaddr = &sockaddr as *const libc::sockaddr_un as *const libc::sockaddr;

    match syscall!(connect(socket, sockaddr, socklen)) {
        Ok(_) => {}
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
        Err(e) => {
            // Close the socket if we hit an error, ignoring the error
            // from closing since we can't pass back two errors.
            let _ = unsafe { libc::close(socket) };

            return Err(e);
        }
    }

    Ok(unsafe { net::UnixStream::from_raw_fd(socket) })
}

pub(crate) fn pair() -> io::Result<(net::UnixStream, net::UnixStream)> {
    let mut fds = [-1; 2];
    let flags = libc::SOCK_STREAM;
    #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "solaris")))]
    let flags = flags | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

    syscall!(socketpair(libc::AF_UNIX, flags, 0, fds.as_mut_ptr()))?;
    let pair = unsafe {
        (
            net::UnixStream::from_raw_fd(fds[0]),
            net::UnixStream::from_raw_fd(fds[1]),
        )
    };

    // Darwin and Solaris do not have SOCK_NONBLOCK or SOCK_CLOEXEC.
    //
    // In order to set those flags, additional `fcntl` sys calls must be
    // performed. If a `fnctl` fails after the sockets have been created,
    // the file descriptors will leak. Creating `pair` above ensures that
    // if there is an error, the file descriptors are closed.
    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
    {
        syscall!(fcntl(fds[0], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC))?;
        syscall!(fcntl(fds[1], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC))?;
    }
    Ok(pair)
}

pub(crate) fn local_addr(socket: &net::UnixStream) -> io::Result<SocketAddr> {
    SocketAddr::new(|sockaddr, socklen| {
        syscall!(getsockname(socket.as_raw_fd(), sockaddr, socklen))
    })
}

pub(crate) fn peer_addr(socket: &net::UnixStream) -> io::Result<SocketAddr> {
    SocketAddr::new(|sockaddr, socklen| {
        syscall!(getpeername(socket.as_raw_fd(), sockaddr, socklen))
    })
}
