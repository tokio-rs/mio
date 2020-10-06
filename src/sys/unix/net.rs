#[cfg(all(feature = "os-poll", any(feature = "tcp", feature = "udp")))]
use std::net::SocketAddr;

#[cfg(all(feature = "os-poll", any(feature = "udp")))]
pub(crate) fn new_ip_socket(
    addr: SocketAddr,
    socket_type: libc::c_int,
) -> std::io::Result<libc::c_int> {
    let domain = match addr {
        SocketAddr::V4(..) => libc::AF_INET,
        SocketAddr::V6(..) => libc::AF_INET6,
    };

    new_socket(domain, socket_type)
}

/// Create a new non-blocking socket.
#[cfg(all(
    feature = "os-poll",
    any(feature = "tcp", feature = "udp", feature = "uds")
))]
pub(crate) fn new_socket(
    domain: libc::c_int,
    socket_type: libc::c_int,
) -> std::io::Result<libc::c_int> {
    #[cfg(any(
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "illumos",
        target_os = "linux",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    let socket_type = socket_type | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

    // Gives a warning for platforms without SOCK_NONBLOCK.
    #[allow(clippy::let_and_return)]
    let socket = syscall!(socket(domain, socket_type, 0));

    // Mimick `libstd` and set `SO_NOSIGPIPE` on apple systems.
    #[cfg(target_vendor = "apple")]
    let socket = socket.and_then(|socket| {
        syscall!(setsockopt(
            socket,
            libc::SOL_SOCKET,
            libc::SO_NOSIGPIPE,
            &1 as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t
        ))
        .map(|_| socket)
    });

    // Darwin doesn't have SOCK_NONBLOCK or SOCK_CLOEXEC. Not sure about
    // Solaris, couldn't find anything online.
    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
    let socket = socket.and_then(|socket| {
        // For platforms that don't support flags in socket, we need to
        // set the flags ourselves.
        syscall!(fcntl(socket, libc::F_SETFL, libc::O_NONBLOCK))
            .and_then(|_| syscall!(fcntl(socket, libc::F_SETFD, libc::FD_CLOEXEC)).map(|_| socket))
            .map_err(|e| {
                // If either of the `fcntl` calls failed, ensure the socket is
                // closed and return the error.
                let _ = syscall!(close(socket));
                e
            })
    });

    socket
}

#[cfg(all(feature = "os-poll", any(feature = "tcp", feature = "udp")))]
pub(crate) fn socket_addr(addr: &SocketAddr) -> (*const libc::sockaddr, libc::socklen_t) {
    use std::mem::size_of_val;

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

/// `storage` must be initialised to `sockaddr_in` or `sockaddr_in6`.
#[cfg(all(feature = "os-poll", feature = "tcp"))]
pub(crate) unsafe fn to_socket_addr(
    storage: *const libc::sockaddr_storage,
) -> std::io::Result<SocketAddr> {
    match (*storage).ss_family as libc::c_int {
        libc::AF_INET => Ok(SocketAddr::V4(
            *(storage as *const libc::sockaddr_in as *const _),
        )),
        libc::AF_INET6 => Ok(SocketAddr::V6(
            *(storage as *const libc::sockaddr_in6 as *const _),
        )),
        _ => Err(std::io::ErrorKind::InvalidInput.into()),
    }
}
