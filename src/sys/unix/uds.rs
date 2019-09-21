use crate::sys::unix::net::new_socket;

use std::cmp::Ordering;
use std::io;
use std::mem;
use std::os::unix::net::{UnixDatagram, UnixListener, UnixStream};
use std::os::unix::prelude::*;
use std::path::Path;

pub fn connect_stream(path: &Path) -> io::Result<UnixStream> {
    let socket = new_socket(libc::AF_UNIX, libc::SOCK_STREAM)?;
    let (raw_addr, raw_addr_length) = socket_addr(path)?;
    let raw_addr = &raw_addr as *const _ as *const _;

    match syscall!(connect(socket, raw_addr, raw_addr_length)) {
        Ok(_) => {}
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
        Err(e) => {
            // Close the socket if we hit an error, ignoring the error
            // from closing since we can't pass back two errors.
            let _ = unsafe { libc::close(socket) };

            return Err(e);
        }
    }

    Ok(unsafe { UnixStream::from_raw_fd(socket) })
}

pub fn pair_stream() -> io::Result<(UnixStream, UnixStream)> {
    let mut fds = [0, 0];
    let socket_type = libc::SOCK_STREAM;

    #[cfg(any(
        target_os = "android",
        target_os = "bitrig",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "linux",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    let flags = socket_type | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

    // Gives a warning for platforms without SOCK_NONBLOCK.
    syscall!(socketpair(libc::AF_UNIX, flags, 0, fds.as_mut_ptr()))?;

    // Darwin and Solaris don't have SOCK_NONBLOCK or SOCK_CLOEXEC.
    //
    // For platforms that don't support flags in `socket`, the flags must be
    // set through `fcntl`. The `F_SETFL` command sets the `O_NONBLOCK` bit.
    // The `F_SETFD` command sets the `FD_CLOEXEC` bit.
    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
    {
        syscall!(fcntl(fds[0], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC))?;
        syscall!(fcntl(fds[1], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC))?;
    }
    Ok(unsafe {
        (
            UnixStream::from_raw_fd(fds[0]),
            UnixStream::from_raw_fd(fds[1]),
        )
    })
}

pub fn bind_datagram(path: &Path) -> io::Result<UnixDatagram> {
    let socket = new_socket(libc::AF_UNIX, libc::SOCK_DGRAM)?;
    let (raw_addr, raw_addr_length) = socket_addr(path)?;
    let raw_addr = &raw_addr as *const _ as *const _;

    syscall!(bind(socket, raw_addr, raw_addr_length))?;
    Ok(unsafe { UnixDatagram::from_raw_fd(socket) })
}

// TODO(kleimkuhler): Duplicated from `pair_stream`... this can probably be
// encapsulated by some `Stream::pair(socket_type: libc::c_int)`
pub fn pair_datagram() -> io::Result<(UnixDatagram, UnixDatagram)> {
    let mut fds = [0, 0];
    let socket_type = libc::SOCK_DGRAM;

    #[cfg(any(
        target_os = "android",
        target_os = "bitrig",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "linux",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    let flags = socket_type | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

    // Gives a warning for platforms without SOCK_NONBLOCK.
    syscall!(socketpair(libc::AF_UNIX, flags, 0, fds.as_mut_ptr()))?;

    // Darwin and Solaris don't have SOCK_NONBLOCK or SOCK_CLOEXEC.
    //
    // For platforms that don't support flags in `socket`, the flags must be
    // set through `fcntl`. The `F_SETFL` command sets the `O_NONBLOCK` bit.
    // The `F_SETFD` command sets the `FD_CLOEXEC` bit.
    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
    {
        syscall!(fcntl(fds[0], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC))?;
        syscall!(fcntl(fds[1], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC))?;
    }
    Ok(unsafe {
        (
            UnixDatagram::from_raw_fd(fds[0]),
            UnixDatagram::from_raw_fd(fds[1]),
        )
    })
}

pub fn unbound_datagram() -> io::Result<UnixDatagram> {
    let socket = UnixDatagram::unbound()?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

pub fn bind_listener(path: &Path) -> io::Result<UnixListener> {
    let socket = new_socket(libc::AF_UNIX, libc::SOCK_STREAM)?;
    let (raw_addr, raw_addr_length) = socket_addr(path)?;
    let raw_addr = &raw_addr as *const _ as *const _;

    syscall!(bind(socket, raw_addr, raw_addr_length))
        .and_then(|_| syscall!(listen(socket, 1024)))
        .map_err(|err| {
            // Close the socket if we hit an error, ignoring the error from
            // closing since we can't pass back two errors.
            let _ = unsafe { libc::close(socket) };
            err
        })
        .map(|_| unsafe { UnixListener::from_raw_fd(socket) })
}

pub fn socket_addr(path: &Path) -> io::Result<(libc::sockaddr_un, libc::socklen_t)> {
    unsafe {
        let mut addr: libc::sockaddr_un = mem::zeroed();
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;

        let bytes = path.as_os_str().as_bytes();

        match (bytes.get(0), bytes.len().cmp(&addr.sun_path.len())) {
            // Abstract paths don't need a null terminator
            (Some(&0), Ordering::Greater) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "path must be no longer than SUN_LEN",
                ));
            }
            (_, Ordering::Greater) | (_, Ordering::Equal) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "path must be shorter than SUN_LEN",
                ));
            }
            _ => {}
        }
        for (dst, src) in addr.sun_path.iter_mut().zip(bytes.iter()) {
            *dst = *src as libc::c_char;
        }
        // null byte for pathname addresses is already there because we zeroed the
        // struct

        let mut len = sun_path_offset() + bytes.len();

        match bytes.get(0) {
            Some(&0) | None => {}
            Some(_) => len += 1,
        }

        Ok((addr, len as libc::socklen_t))
    }
}

fn sun_path_offset() -> usize {
    unsafe {
        // Work with an actual instance of the type since using a null pointer is UB
        let addr: libc::sockaddr_un = mem::uninitialized();
        let base = &addr as *const _ as usize;
        let path = &addr.sun_path as *const _ as usize;
        path - base
    }
}
