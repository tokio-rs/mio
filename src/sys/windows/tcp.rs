use std::io;
use std::net::{self, SocketAddr};
use std::os::windows::io::FromRawSocket;
use std::os::windows::raw::SOCKET as StdSocket; // winapi uses usize, stdlib uses u32/u64.

use winapi::um::winsock2::{
    self,
    closesocket, PF_INET, PF_INET6, SOCKET, SOCKET_ERROR, SOCK_STREAM,
};

use crate::sys::windows::net::{init, new_socket, socket_addr};

pub(crate) type TcpSocket = SOCKET;

pub(crate) fn new_v4_socket() -> io::Result<TcpSocket> {
    init();
    new_socket(PF_INET, SOCK_STREAM)
}

pub(crate) fn new_v6_socket() -> io::Result<TcpSocket> {
    init();
    new_socket(PF_INET6, SOCK_STREAM)
}

pub(crate) fn bind(socket: TcpSocket, addr: SocketAddr) -> io::Result<()> {
    use winsock2::bind;

    let (raw_addr, raw_addr_length) = socket_addr(&addr);
    syscall!(
        bind(socket, raw_addr, raw_addr_length),
        PartialEq::eq,
        SOCKET_ERROR
    )?;
    Ok(())
}

pub(crate) fn connect(socket: TcpSocket, addr: SocketAddr) -> io::Result<net::TcpStream> {
    use winsock2::connect;

    let (raw_addr, raw_addr_length) = socket_addr(&addr);

    let res = syscall!(
        connect(socket, raw_addr, raw_addr_length),
        PartialEq::eq,
        SOCKET_ERROR
    );

    match res {
        Err(err) if err.kind() != io::ErrorKind::WouldBlock => {
            Err(err)
        }
        _ => {
            Ok(unsafe { net::TcpStream::from_raw_socket(socket as StdSocket) })
        }
    }
}

pub(crate) fn listen(socket: TcpSocket, backlog: u32) -> io::Result<net::TcpListener> {
    use winsock2::listen;
    use std::convert::TryInto;

    let backlog = backlog.try_into().unwrap_or(i32::max_value());
    syscall!(listen(socket, backlog), PartialEq::eq, SOCKET_ERROR)?;
    Ok(unsafe { net::TcpListener::from_raw_socket(socket as StdSocket) })
}

pub(crate) fn close(socket: TcpSocket) {
    let _ = unsafe { closesocket(socket) };
}

pub(crate) fn set_reuseaddr(socket: TcpSocket, reuseaddr: bool) -> io::Result<()> {
    use winapi::ctypes::{c_char, c_int};
    use winapi::shared::minwindef::{BOOL, TRUE, FALSE};
    use winapi::um::winsock2::{setsockopt, SOCKET_ERROR, SO_REUSEADDR, SOL_SOCKET};
    use std::mem::size_of;

    let val: BOOL = if reuseaddr { TRUE } else { FALSE };

    match unsafe { setsockopt(
        socket,
        SOL_SOCKET,
        SO_REUSEADDR,
        &val as *const _ as *const c_char,
        size_of::<BOOL>() as c_int,
    ) } {
        0 => Ok(()),
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => panic!("unexpected return value"),
    }
}

pub(crate) fn accept(listener: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    // The non-blocking state of `listener` is inherited. See
    // https://docs.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept#remarks.
    listener.accept()
}
