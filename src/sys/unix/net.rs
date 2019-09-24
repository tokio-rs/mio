use std::io;
use std::mem::size_of_val;
use std::net::SocketAddr;

pub fn new_ip_socket(addr: SocketAddr, socket_type: libc::c_int) -> io::Result<libc::c_int> {
    let domain = match addr {
        SocketAddr::V4(..) => libc::AF_INET,
        SocketAddr::V6(..) => libc::AF_INET6,
    };

    new_socket(domain, socket_type)
}

/// Create a new non-blocking socket.
pub fn new_socket(domain: libc::c_int, socket_type: libc::c_int) -> io::Result<libc::c_int> {
    #[cfg(any(
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "linux",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    let socket_type = socket_type | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

    // Gives a warning for platforms without SOCK_NONBLOCK.
    #[allow(clippy::let_and_return)]
    let socket = syscall!(socket(domain, socket_type, 0));

    // Darwin doesn't have SOCK_NONBLOCK or SOCK_CLOEXEC. Not sure about
    // Solaris, couldn't find anything online.
    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
    let socket = socket.and_then(|socket| {
        // For platforms that don't support flags in socket, we need to
        // set the flags ourselves.
        syscall!(fcntl(socket, libc::F_SETFL, libc::O_NONBLOCK,))
            .and_then(|_| syscall!(fcntl(socket, libc::F_SETFD, libc::FD_CLOEXEC,)).map(|_| socket))
            .map_err(|e| {
                // If either of the `fcntl` calls failed, ensure the socket is
                // closed and return the error.
                let _ = syscall!(close(socket));
                e
            })
    });

    socket
}

pub fn socket_addr(addr: &SocketAddr) -> (*const libc::sockaddr, libc::socklen_t) {
    match addr {
        SocketAddr::V4(ref addr) => (
            addr as *const _ as *const libc::sockaddr,
            size_of_val(addr) as libc::socklen_t,
        ),
        SocketAddr::V6(ref addr) => (
            addr as *const _ as *const libc::sockaddr,
            size_of_val(addr) as libc::socklen_t,
        ),
    }
}
