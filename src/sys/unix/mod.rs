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

mod sourcefd;
pub use self::sourcefd::SourceFd;

#[cfg(feature = "tcp")]
mod tcp;
#[cfg(feature = "tcp")]
pub use self::tcp::{TcpListener, TcpStream};

#[cfg(feature = "udp")]
mod udp;
#[cfg(feature = "udp")]
pub use self::udp::UdpSocket;

#[cfg(feature = "uds")]
mod uds;
#[cfg(feature = "uds")]
pub use self::uds::{SocketAddr, UnixDatagram, UnixListener, UnixStream};

mod waker;
pub use self::waker::Waker;
