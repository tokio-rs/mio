use std::io;
use std::mem;
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, OwnedSocket, RawSocket};

use windows_sys::Win32::Networking::WinSock::{self as ws, INVALID_SOCKET, SOCKADDR_UN};

use super::{cvt, last_error, new_socket, Socket, SocketAddr};
use crate::io_source::IoSource;
use crate::net::UnixStream;

/// Creates and binds an `AF_UNIX` listening socket in non-blocking mode to
/// the given `SocketAddr`.
pub(crate) fn bind_addr(addr: &SocketAddr) -> io::Result<OwnedSocket> {
    let socket = new_socket()?;
    let raw = socket.as_raw_socket();

    unsafe {
        cvt(ws::bind(
            raw as _,
            &addr.addr as *const _ as *const _,
            addr.len,
        ))?;
        cvt(ws::listen(raw as _, 128))?;
    }

    Ok(socket)
}

/// Accept a new connection from `listener`, returning a non-blocking
/// `UnixStream` and the peer's address.
pub(crate) fn accept(listener: &IoSource<Socket>) -> io::Result<(UnixStream, SocketAddr)> {
    listener.do_io(|socket| {
        let (client, addr) = accept_raw(socket.as_raw_socket())?;
        // SAFETY: `accept_raw` returned a valid, connected, non-blocking
        // AF_UNIX socket.
        let stream = unsafe {
            <UnixStream as FromRawSocket>::from_raw_socket(
                <OwnedSocket as IntoRawSocket>::into_raw_socket(client),
            )
        };
        Ok((stream, addr))
    })
}

fn accept_raw(raw: RawSocket) -> io::Result<(OwnedSocket, SocketAddr)> {
    let mut storage: SOCKADDR_UN = unsafe { mem::zeroed() };
    let mut len = mem::size_of::<SOCKADDR_UN>() as i32;

    let client = unsafe { ws::accept(raw as _, &mut storage as *mut _ as *mut _, &mut len) };
    if client == INVALID_SOCKET {
        return Err(last_error());
    }

    let client_raw = client as RawSocket;
    // Accepted sockets inherit the listener's non-blocking mode on Windows,
    // but set `FIONBIO` explicitly to uphold mio's non-blocking contract
    // regardless of how the listening socket was obtained.
    let mut nonblocking: u32 = 1;
    if let Err(err) = cvt(unsafe { ws::ioctlsocket(client as _, ws::FIONBIO, &mut nonblocking) }) {
        unsafe { ws::closesocket(client as _) };
        return Err(err);
    }

    let addr = SocketAddr::from_parts(storage, len)?;
    // SAFETY: WinSock returned a valid socket on success.
    Ok((unsafe { OwnedSocket::from_raw_socket(client_raw) }, addr))
}
