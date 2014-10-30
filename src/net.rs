use std::fmt;
use std::from_str::FromStr;
use io::{IoHandle, NonBlock};
use error::MioResult;
use buf::{Buf, MutBuf};
use os;

pub use std::io::net::ip::{IpAddr, Port};
pub use std::io::net::ip::Ipv4Addr as IPv4Addr;
pub use std::io::net::ip::Ipv6Addr as IPv6Addr;

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

    fn set_reuseport(&self, val: bool) -> MioResult<()> {
        os::set_reuseport(self.desc(), val)
    }
}

pub trait MulticastSocket : Socket {
    fn join_multicast_group(&self, addr: &IpAddr, interface: &Option<IpAddr>) -> MioResult<()> {
        os::join_multicast_group(self.desc(), addr, interface)
    }

    fn leave_multicast_group(&self, addr: &IpAddr, interface: &Option<IpAddr>) -> MioResult<()> {
        os::leave_multicast_group(self.desc(), addr, interface)
    }

    fn set_multicast_ttl(&self, val: u8) -> MioResult<()> {
        os::set_multicast_ttl(self.desc(), val)
    }
}

pub trait UnconnectedSocket {
    fn send_to(&mut self, buf: &mut Buf, tgt: &SockAddr) -> MioResult<NonBlock<()>>;
    fn recv_from(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<SockAddr>>;
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

    pub fn family(&self) -> AddressFamily {
        match *self {
            UnixAddr(..) => Unix,
            InetAddr(IPv4Addr(..), _) => Inet,
            InetAddr(IPv6Addr(..), _) => Inet6
        }
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

pub enum SocketType {
    Dgram,
    Stream,
}

pub mod tcp {
    use os;
    use error::MioResult;
    use buf::{Buf, MutBuf};
    use io;
    use io::{IoHandle, IoAcceptor, IoReader, IoWriter, NonBlock, Ready, WouldBlock};
    use net::{AddressFamily, Socket, SockAddr, Inet, Inet6, Stream};

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
            Ok(TcpSocket { desc: try!(os::socket(family, Stream)) })
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

    impl TcpAcceptor {
        pub fn new(addr: &SockAddr, backlog: uint) -> MioResult<TcpAcceptor> {
            let sock = try!(TcpSocket::new(addr.family()));
            let listener = try!(sock.bind(addr));
            listener.listen(backlog)
        }
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

pub mod udp {
    use os;
    use error::MioResult;
    use buf::{Buf, MutBuf};
    use io::{IoHandle, IoReader, IoWriter, NonBlock, Ready, WouldBlock};
    use net::{AddressFamily, Socket, MulticastSocket, SockAddr, Inet, Dgram};
    use super::UnconnectedSocket;

    #[deriving(Show)]
    pub struct UdpSocket {
        desc: os::IoDesc
    }

    impl UdpSocket {
        pub fn v4() -> MioResult<UdpSocket> {
            UdpSocket::new(Inet)
        }

        fn new(family: AddressFamily) -> MioResult<UdpSocket> {
            Ok(UdpSocket { desc: try!(os::socket(family, Dgram)) })
        }

        pub fn bind(&self, addr: &SockAddr) -> MioResult<()> {
            try!(os::bind(&self.desc, addr))
            Ok(())
        }

        pub fn connect(&self, addr: &SockAddr) -> MioResult<bool> {
            os::connect(&self.desc, addr)
        }

        pub fn bound(addr: &SockAddr) -> MioResult<UdpSocket> {
            let sock = try!(UdpSocket::new(addr.family()));
            try!(sock.bind(addr));
            Ok(sock)
        }
    }

    impl IoHandle for UdpSocket {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl Socket for UdpSocket {
    }

    impl MulticastSocket for UdpSocket {
    }

    impl IoReader for UdpSocket {
        fn read(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<()>> {
            match os::read(&self.desc, buf.mut_bytes()) {
                Ok(cnt) => {
                    buf.advance(cnt);
                    Ok(Ready(()))
                }
                Err(e) => {
                    if e.is_would_block() {
                        Ok(WouldBlock)
                    } else {
                        Err(e)
                    }
                }
            }
        }
    }

    impl IoWriter for UdpSocket {
        fn write(&mut self, buf: &mut Buf) -> MioResult<NonBlock<()>> {
            match os::write(&self.desc, buf.bytes()) {
                Ok(cnt) => {
                    buf.advance(cnt);
                    Ok(Ready(()))
                }
                Err(e) => {
                    if e.is_would_block() {
                        Ok(WouldBlock)
                    } else {
                        Err(e)
                    }
                }
            }
        }
    }

    // Unconnected socket sender -- trait unique to sockets
    impl UnconnectedSocket for UdpSocket {
        fn send_to(&mut self, buf: &mut Buf, tgt: &SockAddr) -> MioResult<NonBlock<()>> {
            match os::sendto(&self.desc, buf.bytes(), tgt) {
                Ok(cnt) => {
                    buf.advance(cnt);
                    Ok(Ready(()))
                }
                Err(e) => {
                    if e.is_would_block() {
                        Ok(WouldBlock)
                    } else {
                        Err(e)
                    }
                }
            }
        }

        fn recv_from(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<SockAddr>> {
            match os::recvfrom(&self.desc, buf.mut_bytes()) {
                Ok((cnt, saddr)) => {
                    buf.advance(cnt);
                    Ok(Ready(saddr))
                }
                Err(e) => {
                    if e.is_would_block() {
                        Ok(WouldBlock)
                    } else {
                        Err(e)
                    }
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

