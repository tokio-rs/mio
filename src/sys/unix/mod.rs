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
mod net;
mod tcp;
mod udp;
mod uds;

pub use self::awakener::Awakener;
pub use self::eventedfd::EventedFd;
pub use self::io::{Io, set_nonblock};
pub use self::tcp::{TcpStream, TcpListener};
pub use self::udp::UdpSocket;
pub use self::uds::UnixSocket;

use std::os::unix::io::FromRawFd;

pub fn pipe() -> ::io::Result<(Io, Io)> {
    use nix::fcntl::{O_NONBLOCK, O_CLOEXEC};
    use nix::unistd::pipe2;

    let (rd, wr) = try!(pipe2(O_NONBLOCK | O_CLOEXEC)
        .map_err(from_nix_error));

    unsafe {
        Ok((Io::from_raw_fd(rd), Io::from_raw_fd(wr)))
    }
}

pub fn from_nix_error(err: ::nix::Error) -> ::io::Error {
    ::io::Error::from_raw_os_error(err.errno() as i32)
}

mod nix {
    pub use nix::{
        c_int,
        Error,
    };
    pub use nix::errno::{EINPROGRESS, EAGAIN};
    pub use nix::fcntl::{fcntl, FcntlArg, O_NONBLOCK};
    pub use nix::sys::socket::{
        sockopt,
        AddressFamily,
        SockAddr,
        SockType,
        SockLevel,
        InetAddr,
        Ipv4Addr,
        Ipv6Addr,
        ControlMessage,
        CmsgSpace,
        MSG_DONTWAIT,
        SOCK_NONBLOCK,
        SOCK_CLOEXEC,
        accept4,
        bind,
        connect,
        getpeername,
        getsockname,
        getsockopt,
        ip_mreq,
        ipv6_mreq,
        linger,
        listen,
        recvfrom,
        recvmsg,
        sendto,
        sendmsg,
        setsockopt,
        socket,
        shutdown,
        Shutdown,
    };
    pub use nix::sys::time::TimeVal;
    pub use nix::sys::uio::IoVec;
    pub use nix::unistd::{
        read,
        write,
        dup,
    };
}

pub struct IoVec<T>(nix::IoVec<T>);

impl<'a> IoVec<&'a [u8]> {
    pub fn from_slice(buf: &'a [u8]) -> IoVec<&'a [u8]> {
        IoVec(nix::IoVec::from_slice(buf))
    }
}

impl<'a> IoVec<&'a mut [u8]> {
    pub fn from_mut(buf: &'a mut [u8]) -> IoVec<&'a mut [u8]> {
        IoVec(nix::IoVec::from_mut_slice(buf))
    }
}
