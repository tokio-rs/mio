use std::fmt;
use std::path::Path;
use std::from_str::FromStr;
use error::MioResult;
use io::IoAcceptor;
use os;

// TODO: A lot of this will most likely get moved into OS specific files

pub use std::io::net::ip::{IpAddr, Port};
pub use std::io::net::ip::Ipv4Addr as IpV4Addr;

// Types of sockets
pub enum AddressFamily {
    Inet,
    Inet6,
    Unix,
}

pub trait IoHandle {
    fn desc(&self) -> os::IoDesc;
}

pub trait Socket : IoHandle {
    // Various sock opt fns
    fn is_acceptor(&self) -> bool {
        unimplemented!();
    }
}

pub struct TcpSocket {
    desc: os::IoDesc
}

impl TcpSocket {
    pub fn v4() -> MioResult<TcpSocket> {
        TcpSocket::new(Inet)
    }

    pub fn v6() -> MioResult<TcpSocket> {
        TcpSocket::new(Inet6)
    }

    fn new(family: AddressFamily) -> MioResult<TcpSocket> {
        Ok(TcpSocket { desc: try!(os::socket(family)) })
    }

    pub fn bind(self, addr: &SockAddr) -> MioResult<TcpAcceptor> {
        try!(os::bind(self.desc, addr))
        Ok(TcpAcceptor { desc: self.desc })
    }
}

impl IoHandle for TcpSocket {
    fn desc(&self) -> os::IoDesc {
        self.desc
    }
}

impl Socket for TcpSocket {
}

pub struct TcpAcceptor {
    desc: os::IoDesc
}

impl IoHandle for TcpAcceptor {
    fn desc(&self) -> os::IoDesc {
        self.desc
    }
}

impl Socket for TcpAcceptor {
}

impl IoAcceptor<TcpSocket> for TcpAcceptor {
    fn accept(&mut self) -> MioResult<TcpSocket> {
        Ok(TcpSocket {
            desc: try!(os::accept(self.desc()))
        })
    }
}

pub struct UnixSocket {
    desc: os::IoDesc
}

impl IoHandle for UnixSocket {
    fn desc(&self) -> os::IoDesc {
        self.desc
    }
}

impl Socket for UnixSocket {
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
