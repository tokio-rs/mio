/// Helper macro to execute a system call that returns an `io::Result`.
///
/// NOTE: this is not a part of the API, do not use!
//
// Macro must be defined before any modules that uses them.
#[macro_export] // Used in `list_event_details`.
#[doc(hidden)]
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

mod net;
mod sourcefd;
mod tcp_listener;
mod tcp_stream;
mod udp;
mod waker;

pub use self::sourcefd::SourceFd;
pub use self::tcp_listener::TcpListener;
pub use self::tcp_stream::TcpStream;
pub use self::udp::UdpSocket;
pub use self::waker::Waker;

pub type Events = Vec<Event>;
