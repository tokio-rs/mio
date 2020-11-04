use std::io;
use std::convert::TryInto;
use std::mem::size_of;
use std::net::{self, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::Duration;
use std::ptr;
use std::os::windows::io::FromRawSocket;
use std::os::windows::raw::SOCKET as StdSocket; // winapi uses usize, stdlib uses u32/u64.

use winapi::ctypes::{c_char, c_int, c_ushort, c_ulong};
use winapi::shared::ws2def::{SOCKADDR_STORAGE, AF_INET, SOCKADDR_IN};
use winapi::shared::ws2ipdef::SOCKADDR_IN6_LH;
use winapi::shared::mstcpip;

use winapi::shared::minwindef::{BOOL, TRUE, FALSE, DWORD, LPVOID, LPDWORD};
use winapi::um::winsock2::{
    self, closesocket, linger, setsockopt, getsockopt, getsockname, PF_INET, PF_INET6, SOCKET, SOCKET_ERROR,
    SOCK_STREAM, SOL_SOCKET, SO_LINGER, SO_REUSEADDR, SO_RCVBUF, SO_SNDBUF, SO_KEEPALIVE, WSAIoctl, LPWSAOVERLAPPED
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
    let val: BOOL = if reuseaddr { TRUE } else { FALSE };

    match unsafe { setsockopt(
        socket,
        SOL_SOCKET,
        SO_REUSEADDR,
        &val as *const _ as *const c_char,
        size_of::<BOOL>() as c_int,
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}

pub(crate) fn get_reuseaddr(socket: TcpSocket) -> io::Result<bool> {
    let mut optval: c_char = 0;
    let mut optlen = size_of::<BOOL>() as c_int;

    match unsafe { getsockopt(
        socket,
        SOL_SOCKET,
        SO_REUSEADDR,
        &mut optval as *mut _ as *mut _,
        &mut optlen,
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => Ok(optval != 0),
    }
}

pub(crate) fn get_localaddr(socket: TcpSocket) -> io::Result<SocketAddr> {
    let mut addr: SOCKADDR_STORAGE = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of_val(&addr) as c_int;

    match unsafe { getsockname(
        socket,
        &mut addr as *mut _ as *mut _,
        &mut length
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => {
            let storage: *const SOCKADDR_STORAGE = (&addr) as *const _;
            if addr.ss_family as c_int == AF_INET {
                let sock_addr : SocketAddrV4 = unsafe { *(storage as *const SOCKADDR_IN as *const _) };
                Ok(sock_addr.into())
            } else {
                let sock_addr : SocketAddrV6 = unsafe { *(storage as *const SOCKADDR_IN6_LH as *const _) };
                Ok(sock_addr.into())
            }
        },
    }


}

pub(crate) fn set_linger(socket: TcpSocket, dur: Option<Duration>) -> io::Result<()> {
    let val: linger = linger {
        l_onoff: if dur.is_some() { 1 } else { 0 },
        l_linger: dur.map(|dur| dur.as_secs() as c_ushort).unwrap_or_default(),
    };

    match unsafe { setsockopt(
        socket,
        SOL_SOCKET,
        SO_LINGER,
        &val as *const _ as *const c_char,
        size_of::<linger>() as c_int,
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}


pub(crate) fn set_recv_buffer_size(socket: TcpSocket, size: u32) -> io::Result<()> {
    let size = size.try_into().ok().unwrap_or_else(i32::max_value);
    match unsafe { setsockopt(
        socket,
        SOL_SOCKET,
        SO_RCVBUF,
        &size as *const _ as *const c_char,
        size_of::<c_int>() as c_int
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}

pub(crate) fn get_recv_buffer_size(socket: TcpSocket) -> io::Result<u32> {
    let mut optval: c_int = 0;
    let mut optlen = size_of::<c_int>() as c_int;
    match unsafe { getsockopt(
        socket,
        SOL_SOCKET,
        SO_RCVBUF,
        &mut optval as *mut _ as *mut _,
        &mut optlen as *mut _,
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => Ok(optval as u32),
    }
}

pub(crate) fn set_send_buffer_size(socket: TcpSocket, size: u32) -> io::Result<()> {
    let size = size.try_into().ok().unwrap_or_else(i32::max_value);
    match unsafe { setsockopt(
        socket,
        SOL_SOCKET,
        SO_SNDBUF,
        &size as *const _ as *const c_char,
        size_of::<c_int>() as c_int
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}

pub(crate) fn get_send_buffer_size(socket: TcpSocket) -> io::Result<u32> {
    let mut optval: c_int = 0;
    let mut optlen = size_of::<c_int>() as c_int;
    match unsafe { getsockopt(
        socket,
        SOL_SOCKET,
        SO_SNDBUF,
        &mut optval as *mut _ as *mut _,
        &mut optlen as *mut _,
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => Ok(optval as u32),
    }
}

pub(crate) fn set_keepalive(socket: TcpSocket, keepalive: bool) -> io::Result<()> {
    let val: BOOL = if keepalive { TRUE } else { FALSE };
    match unsafe { setsockopt(
        socket,
        SOL_SOCKET,
        SO_KEEPALIVE,
        &val as *const _ as *const c_char,
        size_of::<BOOL>() as c_int
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}

pub(crate) fn get_keepalive(socket: TcpSocket) -> io::Result<bool> {
    let mut optval: c_char = 0;
    let mut optlen = size_of::<BOOL>() as c_int;

    match unsafe { getsockopt(
        socket,
        SOL_SOCKET,
        SO_KEEPALIVE,
        &mut optval as *mut _ as *mut _,
        &mut optlen,
    ) } {
        SOCKET_ERROR => Err(io::Error::last_os_error()),
        _ => Ok(optval != FALSE as c_char),
    }
}

pub(crate) fn set_keepalive_time(socket: TcpSocket, time: Duration) -> io::Result<()> {
    let mut keepalive = mstcpip::tcp_keepalive {
        onoff: 0,
        keepalivetime: 0,
        keepaliveinterval: 0,
    };
    // First, populate an empty keepalive structure with the current values.
    // Otherwise, if we call `WSAIoctl` with fields other than the keepalive
    // time set to 0, we'll clobber the existing values.
    get_keepalive_vals(socket, &mut keepalive)?;

    // Windows takes the keepalive time as a u32 of milliseconds.
    let time_ms = time.as_millis().try_into().ok().unwrap_or_else(u32::max_value);
    keepalive.keepalivetime = time_ms as c_ulong;
    // XXX(eliza): if keepalive is disabled on the socket, do we want to turn it
    // on here, or just propagate the OS error?
    set_keepalive_vals(socket, &keepalive)
}

pub(crate) fn get_keepalive_time(socket: TcpSocket) -> io::Result<Option<Duration>> {
    let mut keepalive = mstcpip::tcp_keepalive {
        onoff: 0,
        keepalivetime: 0,
        keepaliveinterval: 0,
    };

    get_keepalive_vals(socket, &mut keepalive)?;

    if keepalive.onoff == 0 {
        // Keepalive is disabled on this socket.
        return Ok(None);
    }

    Ok(Some(Duration::from_millis(keepalive.keepalivetime as u64)))
}

pub(crate) fn set_keepalive_interval(socket: TcpSocket, interval: Duration) -> io::Result<()> {
    let mut keepalive = mstcpip::tcp_keepalive {
        onoff: 0,
        keepalivetime: 0,
        keepaliveinterval: 0,
    };

    // First, populate an empty keepalive structure with the current values.
    // Otherwise, if we call `WSAIoctl` with fields other than the keepalive
    // interval set to 0, we'll clobber the existing values.
    get_keepalive_vals(socket, &mut keepalive)?;

    // Windows takes the keepalive interval as a u32 of milliseconds.
    let interval_ms = interval.as_millis().try_into().ok().unwrap_or_else(u32::max_value);
    keepalive.keepaliveinterval = interval_ms as c_ulong;
    // XXX(eliza): if keepalive is disabled on the socket, do we want to turn it
    // on here, or just propagate the OS error?
    set_keepalive_vals(socket, &keepalive)
}

pub(crate) fn get_keepalive_interval(socket: TcpSocket) -> io::Result<Option<Duration>> {
    let mut keepalive = mstcpip::tcp_keepalive {
        onoff: 0,
        keepalivetime: 0,
        keepaliveinterval: 0,
    };

    get_keepalive_vals(socket, &mut keepalive)?;

    if keepalive.onoff == 0 {
        // Keepalive is disabled on this socket.
        return Ok(None);
    }

    Ok(Some(Duration::from_millis(keepalive.keepaliveinterval as u64)))
}

fn get_keepalive_vals(socket: TcpSocket, vals: &mut mstcpip::tcp_keepalive) -> io::Result<()> {
    match unsafe { WSAIoctl(
        socket,
        mstcpip::SIO_KEEPALIVE_VALS,
        ptr::null_mut() as LPVOID,
        0,
        vals as *mut _ as LPVOID,
        size_of::<mstcpip::tcp_keepalive>() as DWORD,
        ptr::null_mut() as LPDWORD,
        ptr::null_mut() as LPWSAOVERLAPPED,
        None,
    ) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error())
    }
}

fn set_keepalive_vals(socket: TcpSocket, vals: &mstcpip::tcp_keepalive) -> io::Result<()> {
    let vals = vals as *const _ as *mut mstcpip::tcp_keepalive;
    println!("{:p}", vals);
    let mut out = 0;
    match unsafe { WSAIoctl(
        socket,
        mstcpip::SIO_KEEPALIVE_VALS,
        vals as LPVOID,
        size_of::<mstcpip::tcp_keepalive>() as DWORD,
        ptr::null_mut() as LPVOID,
        0 as DWORD,
        &mut out as *mut _ as LPDWORD,
        0 as LPWSAOVERLAPPED,
        None,
    ) } {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error())
    }
}

pub(crate) fn accept(listener: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    // The non-blocking state of `listener` is inherited. See
    // https://docs.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept#remarks.
    listener.accept()
}
