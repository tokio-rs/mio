use std::io;
use std::os::windows::io::{AsRawSocket, FromRawSocket};
use std::convert::TryInto;
use std::path::Path;
use windows_sys::Win32::Networking::WinSock::{self, SOCKET_ERROR, connect as sys_connect, ioctlsocket, FIONBIO};

use super::{stdnet::{self as net}, socket_addr};
use crate::net::SocketAddr;
use crate::sys::windows::net::{init, new_socket};

pub(crate) fn connect(path: &Path) -> io::Result<net::UnixStream> {
    init();
    let socket = new_socket(WinSock::AF_UNIX.into(), WinSock::SOCK_STREAM)?;
    let (sockaddr, socklen) = socket_addr(path)?;
    let sockaddr = &sockaddr as *const WinSock::sockaddr_un as *const WinSock::SOCKADDR;

    // Put into blocking mode to connect.
    wsa_syscall!(
        ioctlsocket(socket, FIONBIO, &mut 0),
        PartialEq::eq,
        SOCKET_ERROR
    )?;

    match wsa_syscall!(
        sys_connect(socket, sockaddr, socklen as _),
        PartialEq::eq,
        SOCKET_ERROR
    ) {
        Ok(_) => {}
        Err(ref err) if err.raw_os_error() == Some(WinSock::WSAEINPROGRESS) => {}
        Err(e) => {
            // Close the socket if we hit an error, ignoring the error
            // from closing since we can't pass back two errors.
            let _ = unsafe { WinSock::closesocket(socket) };

            return Err(e);
        }
    }
    wsa_syscall!(
        ioctlsocket(socket, FIONBIO, &mut 1),
        PartialEq::eq,
        SOCKET_ERROR
    )?;

    Ok(unsafe { net::UnixStream::from_raw_socket(socket.try_into().unwrap()) })
}

pub(crate) fn pair() -> io::Result<(net::UnixStream, net::UnixStream)> {
    net::UnixStream::pair()
}

pub(crate) fn local_addr(socket: &net::UnixStream) -> io::Result<SocketAddr> {
    super::local_addr(socket.as_raw_socket())
}

pub(crate) fn peer_addr(socket: &net::UnixStream) -> io::Result<SocketAddr> {
    super::peer_addr(socket.as_raw_socket())
}
