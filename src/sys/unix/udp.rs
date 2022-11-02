use crate::sys::unix::net::{new_ip_socket, socket_addr};

use std::io;
use std::mem;
use std::mem::size_of;
use std::net::{self, SocketAddr};
use std::os::unix::io::{AsRawFd, FromRawFd};

pub fn bind(addr: SocketAddr) -> io::Result<net::UdpSocket> {
    // Gives a warning for non Apple platforms.
    #[allow(clippy::let_and_return)]
    let socket = new_ip_socket(addr, libc::SOCK_DGRAM);

    socket.and_then(|socket| {
        // On platforms with Berkeley-derived sockets, this allows to quickly
        // rebind a socket, without needing to wait for the OS to clean up the
        // previous one.
        //
        // On Windows, this allows rebinding sockets which are actively in use,
        // which allows “socket hijacking”, so we explicitly don't set it here.
        // https://docs.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse
        #[cfg(not(windows))]
        set_reuseaddr(socket, true)?;

        let (raw_addr, raw_addr_length) = socket_addr(&addr);
        syscall!(bind(socket, raw_addr.as_ptr(), raw_addr_length))
            .map_err(|err| {
                // Close the socket if we hit an error, ignoring the error
                // from closing since we can't pass back two errors.
                let _ = unsafe { libc::close(socket) };
                err
            })
            .map(|_| unsafe { net::UdpSocket::from_raw_fd(socket) })
    })
}

pub(crate) fn only_v6(socket: &net::UdpSocket) -> io::Result<bool> {
    let mut optval: libc::c_int = 0;
    let mut optlen = mem::size_of::<libc::c_int>() as libc::socklen_t;

    syscall!(getsockopt(
        socket.as_raw_fd(),
        libc::IPPROTO_IPV6,
        libc::IPV6_V6ONLY,
        &mut optval as *mut _ as *mut _,
        &mut optlen,
    ))?;

    Ok(optval != 0)
}

fn set_reuseaddr(socket: i32, reuseaddr: bool) -> io::Result<()> {
    let val: libc::c_int = if reuseaddr { 1 } else { 0 };
    syscall!(setsockopt(
        socket.as_raw_fd(),
        libc::SOL_SOCKET,
        libc::SO_REUSEADDR,
        &val as *const libc::c_int as *const libc::c_void,
        size_of::<libc::c_int>() as libc::socklen_t,
    ))?;
    Ok(())
}
