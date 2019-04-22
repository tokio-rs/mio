mod awakener;
mod eventedfd;
mod io;
mod selector;
mod tcp;
mod udp;

pub use self::awakener::Awakener;
pub use self::eventedfd::EventedFd;
pub use self::io::{Io, set_nonblock};
pub use self::selector::{Events, Selector};
pub use self::tcp::{TcpStream, TcpListener};
pub use self::udp::UdpSocket;

use std::io::{Error, Result};
use std::os::unix::io::{FromRawFd, RawFd};
use syscall::{self, pipe2, O_NONBLOCK, O_CLOEXEC};

pub fn pipe() -> Result<(Io, Io)> {
    let mut fds = [0; 2];
    pipe2(&mut fds, O_NONBLOCK | O_CLOEXEC).map_err(from_syscall_error)?;

    unsafe {
        Ok((Io::from_raw_fd(fds[0] as RawFd), Io::from_raw_fd(fds[1] as RawFd)))
    }
}

pub fn from_syscall_error(err: syscall::Error) -> ::io::Error {
    Error::from_raw_os_error(err.errno as i32)
}
