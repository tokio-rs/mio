use std::io;
use std::mem;
use std::mem::{size_of, MaybeUninit};
use std::net::{self, SocketAddr};
use std::time::Duration;
use std::os::unix::io::{AsRawFd, FromRawFd};

use crate::sys::unix::net::{new_socket, socket_addr, to_socket_addr};

pub type TcpSocket = libc::c_int;

pub(crate) fn new_v4_socket() -> io::Result<TcpSocket> {
    new_socket(libc::AF_INET, libc::SOCK_STREAM)
}

pub(crate) fn new_v6_socket() -> io::Result<TcpSocket> {
    new_socket(libc::AF_INET6, libc::SOCK_STREAM)
}

pub(crate) fn bind(socket: TcpSocket, addr: SocketAddr) -> io::Result<()> {
    let (raw_addr, raw_addr_length) = socket_addr(&addr);
    syscall!(bind(socket, raw_addr, raw_addr_length))?;
    Ok(())
}

pub(crate) fn connect(socket: TcpSocket, addr: SocketAddr) -> io::Result<net::TcpStream> {
    let (raw_addr, raw_addr_length) = socket_addr(&addr);

    match syscall!(connect(socket, raw_addr, raw_addr_length)) {
        Err(err) if err.raw_os_error() != Some(libc::EINPROGRESS) => {
            Err(err)
        }
        _ => {
            Ok(unsafe { net::TcpStream::from_raw_fd(socket) })
        }
    }
}

pub(crate) fn listen(socket: TcpSocket, backlog: u32) -> io::Result<net::TcpListener> {
    use std::convert::TryInto;

    let backlog = backlog.try_into().unwrap_or(i32::max_value());
    syscall!(listen(socket, backlog))?;
    Ok(unsafe { net::TcpListener::from_raw_fd(socket) })
}

pub(crate) fn close(socket: TcpSocket) {
    let _ = unsafe { net::TcpStream::from_raw_fd(socket) };
}

pub(crate) fn set_reuseaddr(socket: TcpSocket, reuseaddr: bool) -> io::Result<()> {
    let val: libc::c_int = if reuseaddr { 1 } else { 0 };
    syscall!(setsockopt(
        socket,
        libc::SOL_SOCKET,
        libc::SO_REUSEADDR,
        &val as *const libc::c_int as *const libc::c_void,
        size_of::<libc::c_int>() as libc::socklen_t,
    )).map(|_| ())
}

pub(crate) fn get_reuseaddr(socket: TcpSocket) -> io::Result<bool> {
    let mut optval: libc::c_int = 0;
    let mut optlen = mem::size_of::<libc::c_int>() as libc::socklen_t;

    syscall!(getsockopt(
        socket,
        libc::SOL_SOCKET,
        libc::SO_REUSEADDR,
        &mut optval as *mut _ as *mut _,
        &mut optlen,
    ))?;

    Ok(optval != 0)
}

#[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
pub(crate) fn set_reuseport(socket: TcpSocket, reuseport: bool) -> io::Result<()> {
    let val: libc::c_int = if reuseport { 1 } else { 0 };

    syscall!(setsockopt(
        socket,
        libc::SOL_SOCKET,
        libc::SO_REUSEPORT,
        &val as *const libc::c_int as *const libc::c_void,
        size_of::<libc::c_int>() as libc::socklen_t,
    )).map(|_| ())
}

#[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
pub(crate) fn get_reuseport(socket: TcpSocket) -> io::Result<bool> {
    let mut optval: libc::c_int = 0;
    let mut optlen = mem::size_of::<libc::c_int>() as libc::socklen_t;

    syscall!(getsockopt(
        socket,
        libc::SOL_SOCKET,
        libc::SO_REUSEPORT,
        &mut optval as *mut _ as *mut _,
        &mut optlen,
    ))?;

    Ok(optval != 0)
}

pub(crate) fn get_localaddr(socket: TcpSocket) -> io::Result<SocketAddr> {
    let mut addr: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let mut length = size_of::<libc::sockaddr_storage>() as libc::socklen_t;

    syscall!(getsockname(
        socket,
        &mut addr as *mut _ as *mut _,
        &mut length
    ))?;

    unsafe { to_socket_addr(&addr) }
}

pub(crate) fn set_linger(socket: TcpSocket, dur: Option<Duration>) -> io::Result<()> {
    let val: libc::linger = libc::linger {
        l_onoff: if dur.is_some() { 1 } else { 0 },
        l_linger: dur.map(|dur| dur.as_secs() as libc::c_int).unwrap_or_default(),
    };
    syscall!(setsockopt(
        socket,
        libc::SOL_SOCKET,
        libc::SO_LINGER,
        &val as *const libc::linger as *const libc::c_void,
        size_of::<libc::linger>() as libc::socklen_t,
    )).map(|_| ())
}

pub fn accept(listener: &net::TcpListener) -> io::Result<(net::TcpStream, SocketAddr)> {
    let mut addr: MaybeUninit<libc::sockaddr_storage> = MaybeUninit::uninit();
    let mut length = size_of::<libc::sockaddr_storage>() as libc::socklen_t;

    // On platforms that support it we can use `accept4(2)` to set `NONBLOCK`
    // and `CLOEXEC` in the call to accept the connection.
    #[cfg(any(
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "illumos",
        target_os = "linux",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    let stream = {
        syscall!(accept4(
            listener.as_raw_fd(),
            addr.as_mut_ptr() as *mut _,
            &mut length,
            libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
        ))
        .map(|socket| unsafe { net::TcpStream::from_raw_fd(socket) })
    }?;

    // But not all platforms have the `accept4(2)` call. Luckily BSD (derived)
    // OSes inherit the non-blocking flag from the listener, so we just have to
    // set `CLOEXEC`.
    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
    let stream = {
        syscall!(accept(
            listener.as_raw_fd(),
            addr.as_mut_ptr() as *mut _,
            &mut length
        ))
        .map(|socket| unsafe { net::TcpStream::from_raw_fd(socket) })
        .and_then(|s| syscall!(fcntl(s.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC)).map(|_| s))
    }?;

    // This is safe because `accept` calls above ensures the address
    // initialised.
    unsafe { to_socket_addr(addr.as_ptr()) }.map(|addr| (stream, addr))
}
