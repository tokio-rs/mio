use std::io;
use std::net::{self, SocketAddr};
use std::os::windows::io::FromRawSocket;
use std::os::windows::raw::SOCKET as StdSocket; // winapi uses usize, stdlib uses u32/u64.

use winapi::um::winsock2::{
    bind as win_bind, closesocket, connect as win_connect, listen, SOCKET_ERROR, SOCK_STREAM,
};

use crate::sys::windows::net::{inaddr_any, init, new_socket, socket_addr};

pub fn connect(addr: SocketAddr) -> io::Result<net::TcpStream> {
    init();
    new_socket(addr, SOCK_STREAM)
        .and_then(|socket| {
            // Required for a future `connect_overlapped` operation to be
            // executed successfully.
            let any_addr = inaddr_any(addr);
            let (raw_addr, raw_addr_length) = socket_addr(&any_addr);
            syscall!(
                win_bind(socket, raw_addr, raw_addr_length),
                PartialEq::eq,
                SOCKET_ERROR
            )
            .and_then(|_| {
                let (raw_addr, raw_addr_length) = socket_addr(&addr);
                syscall!(
                    win_connect(socket, raw_addr, raw_addr_length),
                    PartialEq::eq,
                    SOCKET_ERROR
                )
                .or_else(|err| match err {
                    ref err if err.kind() == io::ErrorKind::WouldBlock => Ok(0),
                    err => Err(err),
                })
            })
            .map(|_| socket)
            .map_err(|err| {
                // Close the socket if we hit an error, ignoring the error
                // from closing since we can't pass back two errors.
                let _ = unsafe { closesocket(socket) };
                err
            })
        })
        .map(|socket| unsafe { net::TcpStream::from_raw_socket(socket as StdSocket) })
}

pub fn bind(addr: SocketAddr) -> io::Result<net::TcpListener> {
    init();
    new_socket(addr, SOCK_STREAM).and_then(|socket| {
        let (raw_addr, raw_addr_length) = socket_addr(&addr);
        syscall!(
            win_bind(socket, raw_addr, raw_addr_length,),
            PartialEq::eq,
            SOCKET_ERROR
        )
        .and_then(|_| syscall!(listen(socket, 1024), PartialEq::eq, SOCKET_ERROR))
        .map_err(|err| {
            // Close the socket if we hit an error, ignoring the error
            // from closing since we can't pass back two errors.
            let _ = unsafe { closesocket(socket) };
            err
        })
        .map(|_| unsafe { net::TcpListener::from_raw_socket(socket as StdSocket) })
    })
}

pub fn accept(listener: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    // The non-blocking state of `listener` is inherited. See
    // https://docs.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept#remarks.
    listener.accept()
}
