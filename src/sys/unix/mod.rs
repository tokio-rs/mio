#[cfg(any(target_os = "linux", target_os = "android"))]
mod epoll;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub use self::epoll::{Events, Selector};

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd",
    target_os = "dragonfly", target_os = "netbsd",))]
mod kqueue;

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd",
    target_os = "dragonfly", target_os = "netbsd",))]
pub use self::kqueue::{Events, Selector};

mod awakener;
mod eventedfd;
mod io;
mod net;
mod socket;
mod tcp;
mod udp;
mod uds;

pub use self::awakener::Awakener;
pub use self::eventedfd::EventedFd;
pub use self::io::Io;
pub use self::socket::Socket;
pub use self::tcp::{TcpStream, TcpListener};
pub use self::udp::UdpSocket;
pub use self::uds::UnixSocket;

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
