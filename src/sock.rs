use std::fmt;
use std::path::Path;
use std::from_str::FromStr;
use nix::fcntl::Fd;
use nix::sys::socket::{
    AddressFamily,
    AF_INET,
    AF_INET6,
    SOCK_STREAM,
    SOCK_NONBLOCK,
    SOCK_CLOEXEC,
    socket
};

use error::MioResult;

pub use std::io::net::ip::{IpAddr, Port};
pub use std::io::net::ip::Ipv4Addr as IpV4Addr;

pub trait Socket {
    fn ident(&self) -> Fd;
}

pub struct TcpSocket {
    ident: Fd
}

impl TcpSocket {
    pub fn v4() -> MioResult<TcpSocket> {
        TcpSocket::new(AF_INET)
    }

    pub fn v6() -> MioResult<TcpSocket> {
        TcpSocket::new(AF_INET6)
    }

    fn new(family: AddressFamily) -> MioResult<TcpSocket> {
        let ident = try!(socket(family, SOCK_STREAM, SOCK_NONBLOCK | SOCK_CLOEXEC));

        Ok(TcpSocket { ident: ident })
    }
}

impl Socket for TcpSocket {
    fn ident(&self) -> Fd {
        self.ident
    }
}

pub struct UnixSocket {
    ident: Fd
}

impl Socket for UnixSocket {
    fn ident(&self) -> Fd {
        self.ident
    }
}

pub enum SockAddr {
    UnixAddr(Path),
    InetAddr(IpAddr, Port)
}

impl SockAddr {
    pub fn parse(s: &str) -> Option<SockAddr> {
        use std::io::net::ip;

        let addr: Option<ip::SocketAddr> = FromStr::from_str(s);
        addr.map(|a| InetAddr(a.ip, a.port))
    }
}

impl FromStr for SockAddr {
    fn from_str(s: &str) -> Option<SockAddr> {
        SockAddr::parse(s)
    }
}

impl fmt::Show for SockAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            InetAddr(ip, port) => write!(fmt, "{}:{}", ip, port),
            _ => write!(fmt, "not implemented")
        }
    }
}
