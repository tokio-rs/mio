/// Helper macro to execute a system call that returns an `io::Result`.
//
// Macro must be defined before any modules that uses them.
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

mod net;

mod selector;
pub use self::selector::{event, Event, Events, Selector};

#[cfg(any(
    all(unix, feature = "tcp"),
    all(unix, feature = "udp"),
    all(unix, feature = "uds"),
    all(unix, feature = "os-ext"),
))]
mod sourcefd;
#[cfg(any(
    all(unix, feature = "tcp"),
    all(unix, feature = "udp"),
    all(unix, feature = "uds"),
    all(unix, feature = "os-ext"),
))]
pub use self::sourcefd::SourceFd;

cfg_tcp! {
    mod tcp;
    pub use self::tcp::{TcpListener, TcpStream};
}

cfg_udp! {
    mod udp;
    pub use self::udp::UdpSocket;
}

cfg_uds! {
    mod uds;
    pub use self::uds::{SocketAddr, UnixDatagram, UnixListener, UnixStream};
}

mod waker;
pub use self::waker::Waker;
