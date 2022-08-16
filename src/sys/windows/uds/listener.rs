use std::{io, mem};
use std::convert::TryInto;
use std::os::windows::io::{AsRawSocket, FromRawSocket};
use std::path::Path;
use std::os::raw::c_int;
use windows_sys::Win32::Networking::WinSock::{
    self,
    SOCKET_ERROR,
    INVALID_SOCKET,
    bind as sys_bind,
    listen,
    accept as sys_accept
};

use super::{stdnet as net, socket_addr};
use crate::net::{SocketAddr, UnixStream};
use crate::sys::windows::net::{init, new_socket};

pub(crate) fn bind(path: &Path) -> io::Result<net::UnixListener> {
    init();
    let socket = new_socket(WinSock::AF_UNIX.into(), WinSock::SOCK_STREAM)?;
    let (sockaddr, socklen) = socket_addr(path)?;
    let sockaddr = &sockaddr as *const WinSock::sockaddr_un as *const WinSock::SOCKADDR;

    wsa_syscall!(sys_bind(socket, sockaddr, socklen as _), PartialEq::eq, SOCKET_ERROR)
        .and_then(|_| wsa_syscall!(listen(socket, 128), PartialEq::eq, SOCKET_ERROR))
        .map_err(|err| {
            // Close the socket if we hit an error, ignoring the error from
            // closing since we can't pass back two errors.
            let _ = unsafe { WinSock::closesocket(socket) };
            err
        })
        .map(|_| unsafe { net::UnixListener::from_raw_socket(socket.try_into().unwrap()) })
}

pub(crate) fn accept(listener: &net::UnixListener) -> io::Result<(UnixStream, SocketAddr)> {
    let sockaddr = mem::MaybeUninit::<WinSock::sockaddr_un>::zeroed();

    // This is safe to assume because a `WinSock::sockaddr_un` filled with `0`
    // bytes is properly initialized.
    //
    // `0` is a valid value for `sockaddr_un::sun_family`; it is
    // `WinSock::AF_UNSPEC`.
    //
    // `[0; 108]` is a valid value for `sockaddr_un::sun_path`; it begins an
    // abstract path.
    let mut sockaddr = unsafe { sockaddr.assume_init() };

    sockaddr.sun_family = WinSock::AF_UNIX;
    let mut socklen = mem::size_of_val(&sockaddr) as c_int;

    let socket = wsa_syscall!(
        sys_accept(
            listener.as_raw_socket().try_into().unwrap(),
            &mut sockaddr as *mut WinSock::sockaddr_un as *mut WinSock::SOCKADDR,
            &mut socklen as _
        ),
        PartialEq::eq,
        INVALID_SOCKET
    );

    socket
        .map(|socket| unsafe { net::UnixStream::from_raw_socket(socket.try_into().unwrap()) })
        .map(UnixStream::from_std)
        .map(|stream| (stream, SocketAddr::from_parts(sockaddr, socklen)))
}

pub(crate) fn local_addr(listener: &net::UnixListener) -> io::Result<SocketAddr> {
    super::local_addr(listener.as_raw_socket())
}