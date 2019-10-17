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
    #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "solaris")))]
    {
        let socket_type = socket_type | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;
        syscall!(socket(domain, socket_type, 0))
    }

    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
    {
        let socket = syscall!(socket(domain, socket_type, 0))?;
        if let Err(e) = (|| {
            syscall!(fcntl(socket, libc::F_SETFL, libc::O_NONBLOCK))?;
            syscall!(fcntl(socket, libc::F_SETFD, libc::FD_CLOEXEC))
        })() {
            // If either of the `fcntl` calls failed, ensure the socket is
            // closed and return the error.
            syscall!(close(socket))?;
            return Err(e);
        }
        Ok(socket)
    }
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
