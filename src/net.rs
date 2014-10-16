use std::fmt;
use std::from_str::FromStr;
use io::{IoHandle};
use error::MioResult;
use os;

pub use std::io::net::ip::{IpAddr, Port};
pub use std::io::net::ip::Ipv4Addr as IpV4Addr;

pub trait Socket : IoHandle {
    fn linger(&self) -> MioResult<uint> {
        os::linger(self.desc())
    }

    fn set_linger(&self, dur_s: uint) -> MioResult<()> {
        os::set_linger(self.desc(), dur_s)
    }

    fn set_reuseaddr(&self, val: bool) -> MioResult<()> {
        os::set_reuseaddr(self.desc(), val)
    }
}

// Types of sockets
pub enum AddressFamily {
    Inet,
    Inet6,
    Unix,
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

pub mod tcp {
    use os;
    use error::MioResult;
    use buf::{Buf, MutBuf};
    use io;
    use io::{IoHandle, IoAcceptor, IoReader, IoWriter, NonBlock, Ready, WouldBlock};
    use net::{AddressFamily, Socket, SockAddr, Inet, Inet6};

    #[deriving(Show)]
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

        pub fn bind(self, addr: &SockAddr) -> MioResult<TcpListener> {
            try!(os::bind(&self.desc, addr))
            Ok(TcpListener { desc: self.desc })
        }
    }

    impl IoHandle for TcpSocket {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl IoReader for TcpSocket {
        fn read(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<()>> {
            io::read(self, buf)
        }
    }

    impl IoWriter for TcpSocket {
        fn write(&mut self, buf: &mut Buf) -> MioResult<NonBlock<()>> {
            io::write(self, buf)
        }
    }

    impl Socket for TcpSocket {
    }

    #[deriving(Show)]
    pub struct TcpListener {
        desc: os::IoDesc,
    }

    impl TcpListener {
        pub fn listen(self, backlog: uint) -> MioResult<TcpAcceptor> {
            try!(os::listen(self.desc(), backlog));
            Ok(TcpAcceptor { desc: self.desc })
        }
    }

    impl IoHandle for TcpListener {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    #[deriving(Show)]
    pub struct TcpAcceptor {
        desc: os::IoDesc,
    }

    impl IoHandle for TcpAcceptor {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl Socket for TcpAcceptor {
    }

    impl IoAcceptor<TcpSocket> for TcpAcceptor {
        fn accept(&mut self) -> MioResult<NonBlock<TcpSocket>> {
            match os::accept(self.desc()) {
                Ok(sock) => Ok(Ready(TcpSocket { desc: sock })),
                Err(e) => {
                    if e.is_would_block() {
                        return Ok(WouldBlock);
                    }

                    return Err(e);
                }
            }
        }
    }
}

pub mod pipe {
    use os;
    use io::{IoHandle};
    use net::Socket;

    #[deriving(Show)]
    pub struct UnixSocket {
        desc: os::IoDesc
    }

    impl IoHandle for UnixSocket {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl Socket for UnixSocket {
    }
}
