use crate::sys::windows::net::init;
use crate::sys::Socket;

use std::io;
use std::net::{self, SocketAddr};
use std::os::windows::io::{FromRawSocket, IntoRawSocket};

use winapi::um::winsock2::SOCK_DGRAM;

pub fn bind(addr: SocketAddr) -> io::Result<net::UdpSocket> {
    init();
    let socket = Socket::from_addr(addr, SOCK_DGRAM, 0)?;
    socket.bind(addr)?;
    Ok(unsafe { net::UdpSocket::from_raw_socket(socket.into_raw_socket()) })
}
