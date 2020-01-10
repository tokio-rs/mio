use super::from_socket_addr;
use crate::net::{SocketAddr, UnixStream};
use crate::sys::Socket;

use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::net;
use std::path::Path;
use std::{io, mem};

pub(crate) fn bind(path: &Path) -> io::Result<net::UnixListener> {
    let socket = Socket::new(libc::AF_UNIX, libc::SOCK_STREAM, 0)?;
    let (storage, len) = from_socket_addr(path)?;
    socket.bind2(
        &storage as *const libc::sockaddr_un as *const libc::sockaddr,
        len,
    )?;
    socket.listen(1024)?;
    Ok(unsafe { net::UnixListener::from_raw_fd(socket.into_raw_fd()) })
}

pub(crate) fn accept(listener: &net::UnixListener) -> io::Result<(UnixStream, SocketAddr)> {
    let socket = unsafe { Socket::from_raw_fd(listener.as_raw_fd()) };
    let storage = mem::MaybeUninit::<libc::sockaddr_un>::zeroed();

    // Safety: A `libc::sockaddr_un` initialized with `0` bytes is properly
    // initialized.
    //
    // `0` is a valid value for `sockaddr_un::sun_family`; it is
    // `libc::AF_UNSPEC`.
    //
    // `[0; 108]` is a valid value for `sockaddr_un::sun_path`; it begins an
    // abstract path.
    let mut storage = unsafe { storage.assume_init() };

    storage.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let len = mem::size_of_val(&storage) as libc::socklen_t;
    let (socket, storage) = socket.accept2(
        &mut storage as *mut libc::sockaddr_un as *mut libc::sockaddr,
        len,
    )?;
    let stream = unsafe { net::UnixStream::from_raw_fd(socket.into_raw_fd()) };
    let addr = unsafe { SocketAddr::from_parts(*(storage as *const libc::sockaddr_un), len) };
    Ok((UnixStream::from_std(stream), addr))
}

pub(crate) fn local_addr(listener: &net::UnixListener) -> io::Result<SocketAddr> {
    super::local_addr(listener.as_raw_fd())
}
