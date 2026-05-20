use std::io;
use std::os::windows::io::{AsRawSocket, OwnedSocket};

use windows_sys::Win32::Networking::WinSock::{self as ws, SOCKET_ERROR};

use super::{last_error, new_socket, SocketAddr};

/// Creates an `AF_UNIX` stream socket and connects it to the given
/// `SocketAddr`. The socket is left in non-blocking mode.
pub(crate) fn connect_addr(addr: &SocketAddr) -> io::Result<OwnedSocket> {
    let socket = new_socket()?;
    let raw = socket.as_raw_socket();

    // The socket is non-blocking; `connect` may return `WSAEWOULDBLOCK`
    // which we treat as success (the user will be notified when the
    // connection completes).
    let res = unsafe { ws::connect(raw as _, &addr.addr as *const _ as *const _, addr.len) };
    if res == SOCKET_ERROR {
        let err = last_error();
        if err.kind() != io::ErrorKind::WouldBlock {
            return Err(err);
        }
    }

    Ok(socket)
}
