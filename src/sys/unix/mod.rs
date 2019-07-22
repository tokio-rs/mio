/// Helper macro to execute a system call that returns an `io::Result`.
//
// Macro must be defined before any modules that uses them.
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
mod epoll;

#[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
pub use self::epoll::{event, Event, Selector};

#[cfg(any(
    target_os = "bitrig",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "ios",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod kqueue;

#[cfg(any(
    target_os = "bitrig",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "ios",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub use self::kqueue::{event, Event, Selector};

mod sourcefd;
mod tcp;
mod udp;
mod waker;

pub use self::sourcefd::SourceFd;
pub use self::tcp::{TcpListener, TcpStream};
pub use self::udp::UdpSocket;
pub use self::waker::Waker;

pub type Events = Vec<Event>;

pub mod net {
    use std::io;
    use std::mem::size_of_val;
    use std::net::SocketAddr;

    /// Create a new non-blocking socket.
    pub fn new_socket(addr: SocketAddr, socket_type: libc::c_int) -> io::Result<libc::c_int> {
        let domain = match addr {
            SocketAddr::V4(..) => libc::AF_INET,
            SocketAddr::V6(..) => libc::AF_INET6,
        };

        #[cfg(any(
            target_os = "android",
            target_os = "bitrig",
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
            syscall!(fcntl(
                socket,
                libc::F_SETFL,
                libc::O_NONBLOCK | libc::O_CLOEXEC
            ))
            .map(|_| socket)
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
}
