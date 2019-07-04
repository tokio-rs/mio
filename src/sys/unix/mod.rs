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

mod eventedfd;
mod io;
mod tcp;
mod udp;
mod uio;
mod waker;

pub use self::eventedfd::EventedFd;
pub use self::tcp::{TcpListener, TcpStream};
pub use self::udp::UdpSocket;
pub use self::waker::Waker;

pub use iovec::IoVec;

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
