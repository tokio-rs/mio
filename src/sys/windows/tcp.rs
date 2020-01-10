use crate::sys::windows::net::{any_socket_addr, init};
use crate::sys::Socket;

use std::io;
use std::net::{self, SocketAddr};
use std::os::windows::io::{FromRawSocket, IntoRawSocket};

use winapi::um::winsock2::SOCK_STREAM;

pub fn connect(addr: SocketAddr) -> io::Result<net::TcpStream> {
    init();
    let socket = Socket::from_addr(addr, SOCK_STREAM, 0)?;

    // Required for a future `connect_overlapped` operation to be executed
    // successfully (todo: ?).
    let any_addr = any_socket_addr(addr);

    socket.bind(any_addr)?;
    socket.connect(addr)?;
    Ok(unsafe { net::TcpStream::from_raw_socket(socket.into_raw_socket()) })
}

pub fn bind(addr: SocketAddr) -> io::Result<net::TcpListener> {
    init();
    let socket = Socket::from_addr(addr, SOCK_STREAM, 0)?;
    socket.bind(addr)?;
    socket.listen(1024)?;
    Ok(unsafe { net::TcpListener::from_raw_socket(socket.into_raw_socket()) })
}

pub fn accept(listener: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    // The non-blocking state of `listener` is inherited. See
    // https://docs.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept#remarks.
    listener.accept()
}
