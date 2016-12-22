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

use std::os::unix::io::FromRawFd;

pub fn pipe() -> ::io::Result<(Io, Io)> {
    use syscall::{O_NONBLOCK, O_CLOEXEC};
    use syscall::pipe2;

    let mut fds = [0; 2];
    try!(pipe2(&mut fds, O_NONBLOCK | O_CLOEXEC).map_err(from_syscall_error));

    unsafe {
        Ok((Io::from_raw_fd(fds[0]), Io::from_raw_fd(fds[1])))
    }
}

pub fn from_syscall_error(err: ::syscall::Error) -> ::io::Error {
    ::io::Error::from_raw_os_error(err.errno as i32)
}
