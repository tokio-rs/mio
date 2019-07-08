/// Helper macro to execute a system call that returns an `io::Result`.
//
// Macro must be defined before any modules that uses them.
macro_rules! syscall {
    ($e:expr) => {{
        let res = unsafe { $e };
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
pub use self::epoll::{event, Event, Events, Selector};

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
pub use self::kqueue::{event, Event, Events, Selector};

mod io;
mod sourcefd;
mod tcp;
mod udp;
mod uio;
mod waker;

pub use self::sourcefd::SourceFd;
pub use self::tcp::{TcpListener, TcpStream};
pub use self::udp::UdpSocket;
pub use self::waker::Waker;

trait IsMinusOne {
    fn is_minus_one(&self) -> bool;
}

impl IsMinusOne for i32 {
    fn is_minus_one(&self) -> bool {
        *self == -1
    }
}
impl IsMinusOne for isize {
    fn is_minus_one(&self) -> bool {
        *self == -1
    }
}

fn cvt<T: IsMinusOne>(t: T) -> std::io::Result<T> {
    use std::io;

    if t.is_minus_one() {
        Err(io::Error::last_os_error())
    } else {
        Ok(t)
    }
}
