use libc::{self, c_int};

#[macro_use]
pub mod dlsym;

#[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
mod epoll;

#[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
pub use self::epoll::{Event, Events, Selector, SysEvent};

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
pub use self::kqueue::{Event, Events, Selector, SysEvent};

mod eventedfd;
mod io;
mod tcp;
mod udp;
mod uio;
mod waker;

pub use self::eventedfd::EventedFd;
pub use self::io::{set_nonblock, Io};
pub use self::tcp::{TcpListener, TcpStream};
pub use self::udp::UdpSocket;
pub use self::waker::Waker;

pub use iovec::IoVec;

use std::os::unix::io::FromRawFd;

#[allow(dead_code)]
pub fn pipe() -> std::io::Result<(Io, Io)> {
    // Use pipe2 for atomically setting O_CLOEXEC if we can, but otherwise
    // just fall back to using `pipe`.
    dlsym!(fn pipe2(*mut c_int, c_int) -> c_int);

    let mut pipes = [0; 2];
    let flags = libc::O_NONBLOCK | libc::O_CLOEXEC;
    unsafe {
        match pipe2.get() {
            Some(pipe2_fn) => {
                cvt(pipe2_fn(pipes.as_mut_ptr(), flags))?;
            }
            None => {
                cvt(libc::pipe(pipes.as_mut_ptr()))?;
                libc::fcntl(pipes[0], libc::F_SETFL, flags);
                libc::fcntl(pipes[1], libc::F_SETFL, flags);
            }
        }
    }

    unsafe { Ok((Io::from_raw_fd(pipes[0]), Io::from_raw_fd(pipes[1]))) }
}

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
