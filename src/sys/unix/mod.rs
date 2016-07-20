#[cfg(any(target_os = "linux", target_os = "android"))]
mod epoll;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub use self::epoll::{Events, Selector};

#[cfg(any(target_os = "bitrig", target_os = "dragonfly",
    target_os = "freebsd", target_os = "ios", target_os = "macos",
    target_os = "netbsd", target_os = "openbsd"))]
mod kqueue;

#[cfg(any(target_os = "bitrig", target_os = "dragonfly",
    target_os = "freebsd", target_os = "ios", target_os = "macos",
    target_os = "netbsd", target_os = "openbsd"))]
pub use self::kqueue::{Events, Selector};

mod awakener;
mod eventedfd;
mod io;
mod tcp;
mod udp;

pub use self::awakener::Awakener;
pub use self::eventedfd::EventedFd;
pub use self::io::{Io, set_nonblock};
pub use self::tcp::{TcpStream, TcpListener};
pub use self::udp::UdpSocket;

pub fn pipe() -> ::io::Result<(Io, Io)> {
    use nix::fcntl::{O_NONBLOCK, O_CLOEXEC};
    use nix::unistd::pipe2;

    let (rd, wr) = try!(pipe2(O_NONBLOCK | O_CLOEXEC)
        .map_err(from_nix_error));

    Ok((Io::from_raw_fd(rd), Io::from_raw_fd(wr)))
}

pub fn from_nix_error(err: ::nix::Error) -> ::io::Error {
    ::io::Error::from_raw_os_error(err.errno() as i32)
}
