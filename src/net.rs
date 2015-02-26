//! Networking primitives
//!
use std::fmt;
use std::str::FromStr;
use std::net::SocketAddr as StdSocketAddr;
use std::path::PathBuf;
use io::{IoHandle, NonBlock};
use error::MioResult;
use buf::{Buf, MutBuf};
use os;

pub use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
pub use std::net::SocketAddr as InetAddr;
pub use nix::sys::socket::{AddressFamily, SockType, ToInAddr};

pub trait Socket : IoHandle {
    fn linger(&self) -> MioResult<usize> {
        os::linger(self.desc())
    }

    fn set_linger(&self, dur_s: usize) -> MioResult<()> {
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
    fn join_multicast_group(&self, addr: &IpAddr, interface: Option<&IpAddr>) -> MioResult<()> {
        os::join_multicast_group(self.desc(), addr, interface)
    }

    fn leave_multicast_group(&self, addr: &IpAddr, interface: Option<&IpAddr>) -> MioResult<()> {
        os::leave_multicast_group(self.desc(), addr, interface)
    }

    fn set_multicast_ttl(&self, val: u8) -> MioResult<()> {
        os::set_multicast_ttl(self.desc(), val)
    }
}

pub trait UnconnectedSocket {

    fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &SockAddr) -> MioResult<NonBlock<()>>;

    fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> MioResult<NonBlock<SockAddr>>;
}

// TODO: Get rid of this type
pub enum SockAddr {
    UnixAddr(PathBuf),
    InetAddr(InetAddr)
}

impl SockAddr {
    pub fn parse(s: &str) -> Option<SockAddr> {
        FromStr::from_str(s).ok()
            .map(|addr: InetAddr| SockAddr::InetAddr(addr))
    }

    pub fn family(&self) -> AddressFamily {
        match *self {
            SockAddr::UnixAddr(..) => AddressFamily::Unix,
            SockAddr::InetAddr(addr) => {
                match addr.ip() {
                    IpAddr::V4(..) => AddressFamily::Inet,
                    IpAddr::V6(..) => AddressFamily::Inet6,
                }
            }
        }
    }

    pub fn from_path(p: PathBuf) -> SockAddr {
        SockAddr::UnixAddr(p)
    }

    #[inline]
    pub fn consume_std(addr: InetAddr) -> SockAddr {
        SockAddr::InetAddr(addr)
    }

    #[inline]
    pub fn from_std(addr: &InetAddr) -> SockAddr {
        SockAddr::InetAddr(addr.clone())
    }

    pub fn to_std(&self) -> Option<InetAddr> {
        match *self {
            SockAddr::InetAddr(ref addr) => {
                Some(addr.clone())
            }
            _ => None
        }
    }

    pub fn into_std(self) -> Option<StdSocketAddr> {
        match self {
            SockAddr::InetAddr(addr) => {
                Some(addr)
            }
            _ => None
        }
    }
}

impl fmt::Debug for SockAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SockAddr::InetAddr(addr) => write!(fmt, "{}", addr),
            _ => write!(fmt, "not implemented")
        }
    }
}


// TODO: Get rid of all this
use nix::sys::socket::{ToSockAddr, FromSockAddr};
use nix::sys::socket::SockAddr as NixSockAddr;
use nix::{NixResult};

impl ToSockAddr for SockAddr {
    fn to_sock_addr(&self) -> NixResult<NixSockAddr> {
        match *self {
            SockAddr::InetAddr(addr) => {
                addr.to_sock_addr()
            }
            SockAddr::UnixAddr(ref path) => {
                path.to_sock_addr()
            }
        }
    }
}

impl FromSockAddr for SockAddr {
    fn from_sock_addr(addr: &NixSockAddr) -> Option<SockAddr> {
        match *addr {
            NixSockAddr::IpV4(..) |
            NixSockAddr::IpV6(..) => {
                FromSockAddr::from_sock_addr(addr)
                    .map(|a: InetAddr| SockAddr::InetAddr(a))
            }
            NixSockAddr::Unix(..) => {
                FromSockAddr::from_sock_addr(addr)
                    .map(|a: PathBuf| SockAddr::UnixAddr(a))
            }
        }
    }
}

/// TCP networking primitives
///
pub mod tcp {
    use os;
    use error::MioResult;
    use buf::{Buf, MutBuf};
    use io;
    use io::{FromIoDesc, IoHandle, IoAcceptor, IoReader, IoWriter, NonBlock};
    use io::NonBlock::{Ready, WouldBlock};
    use net::{AddressFamily, Socket, SockAddr, SockType};

    #[derive(Debug)]
    pub struct TcpSocket {
        desc: os::IoDesc
    }

    impl TcpSocket {
        pub fn v4() -> MioResult<TcpSocket> {
            TcpSocket::new(AddressFamily::Inet)
        }

        pub fn v6() -> MioResult<TcpSocket> {
            TcpSocket::new(AddressFamily::Inet6)
        }

        fn new(family: AddressFamily) -> MioResult<TcpSocket> {
            Ok(TcpSocket { desc: try!(os::socket(family, SockType::Stream)) })
        }

        /// Connects the socket to the specified address. When the operation
        /// completes, the handler will be notified with the supplied token.
        ///
        /// The goal of this method is to ensure that the event loop will always
        /// notify about the connection, even if the connection happens
        /// immediately. Otherwise, every consumer of the event loop would have
        /// to worry about possibly-immediate connection.
        pub fn connect(&self, addr: &SockAddr) -> MioResult<()> {
            debug!("socket connect; addr={:?}", addr);

            // Attempt establishing the context. This may not complete immediately.
            if try!(os::connect(&self.desc, addr)) {
                // On some OSs, connecting to localhost succeeds immediately. In
                // this case, queue the writable callback for execution during the
                // next event loop tick.
                debug!("socket connected immediately; addr={:?}", addr);
            }

            Ok(())
        }

        pub fn bind(self, addr: &SockAddr) -> MioResult<TcpListener> {
            try!(os::bind(&self.desc, addr));
            Ok(TcpListener { desc: self.desc })
        }

        pub fn getpeername(&self) -> MioResult<SockAddr> {
            os::getpeername(&self.desc)
        }

        pub fn getsockname(&self) -> MioResult<SockAddr> {
            os::getsockname(&self.desc)
        }
    }

    impl IoHandle for TcpSocket {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl FromIoDesc for TcpSocket {
        fn from_desc(desc: os::IoDesc) -> Self {
            TcpSocket { desc: desc }
        }
    }

    impl IoReader for TcpSocket {
        fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
            io::read(self, buf)
        }

        fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
            io::read_slice(self, buf)
        }
    }

    impl IoWriter for TcpSocket {
        fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
            io::write(self, buf)
        }

        fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
            io::write_slice(self, buf)
        }
    }

    impl Socket for TcpSocket {
    }

    #[derive(Debug)]
    pub struct TcpListener {
        desc: os::IoDesc,
    }

    impl TcpListener {
        pub fn listen(self, backlog: usize) -> MioResult<TcpAcceptor> {
            try!(os::listen(self.desc(), backlog));
            Ok(TcpAcceptor { desc: self.desc })
        }
    }

    impl IoHandle for TcpListener {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl FromIoDesc for TcpListener {
        fn from_desc(desc: os::IoDesc) -> Self {
            TcpListener { desc: desc }
        }
    }

    #[derive(Debug)]
    pub struct TcpAcceptor {
        desc: os::IoDesc,
    }

    impl TcpAcceptor {
        pub fn new(addr: &SockAddr, backlog: usize) -> MioResult<TcpAcceptor> {
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

    impl FromIoDesc for TcpAcceptor {
        fn from_desc(desc: os::IoDesc) -> Self {
            TcpAcceptor { desc: desc }
        }
    }

    impl Socket for TcpAcceptor {
    }

    impl IoAcceptor for TcpAcceptor {
        type Output = TcpSocket;

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
    use io::{FromIoDesc, IoHandle, IoReader, IoWriter, NonBlock};
    use io::NonBlock::{Ready, WouldBlock};
    use io;
    use net::{AddressFamily, Socket, MulticastSocket, SockAddr, SockType};
    use super::UnconnectedSocket;

    #[derive(Debug)]
    pub struct UdpSocket {
        desc: os::IoDesc
    }

    impl UdpSocket {
        pub fn v4() -> MioResult<UdpSocket> {
            UdpSocket::new(AddressFamily::Inet)
        }

        fn new(family: AddressFamily) -> MioResult<UdpSocket> {
            Ok(UdpSocket { desc: try!(os::socket(family, SockType::Datagram)) })
        }

        pub fn bind(&self, addr: &SockAddr) -> MioResult<()> {
            try!(os::bind(&self.desc, addr));
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

    impl FromIoDesc for UdpSocket {
        fn from_desc(desc: os::IoDesc) -> Self {
            UdpSocket { desc: desc }
        }
    }

    impl Socket for UdpSocket {
    }

    impl MulticastSocket for UdpSocket {
    }

    impl IoReader for UdpSocket {
        fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
            io::read(self, buf)
        }

        fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
            io::read_slice(self, buf)
        }
    }

    impl IoWriter for UdpSocket {
        fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
            io::write(self, buf)
        }

        fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
            io::write_slice(self, buf)
        }
    }

    // Unconnected socket sender -- trait unique to sockets
    impl UnconnectedSocket for UdpSocket {
        fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &SockAddr) -> MioResult<NonBlock<()>> {
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

        fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> MioResult<NonBlock<SockAddr>> {
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

/// Named pipes
pub mod pipe {
    use os;
    use error::MioResult;
    use buf::{Buf, MutBuf};
    use io;
    use io::{FromIoDesc, IoHandle, IoAcceptor, IoReader, IoWriter, NonBlock};
    use io::NonBlock::{Ready, WouldBlock};
    use net::{AddressFamily, Socket, SockAddr, SockType};

    #[derive(Debug)]
    pub struct UnixSocket {
        desc: os::IoDesc
    }

    impl UnixSocket {
        pub fn stream() -> MioResult<UnixSocket> {
            UnixSocket::new(SockType::Stream)
        }

        fn new(socket_type: SockType) -> MioResult<UnixSocket> {
            Ok(UnixSocket { desc: try!(os::socket(AddressFamily::Unix, socket_type)) })
        }

        pub fn connect(&self, addr: &SockAddr) -> MioResult<()> {
            debug!("socket connect; addr={:?}", addr);

            // Attempt establishing the context. This may not complete immediately.
            if try!(os::connect(&self.desc, addr)) {
                // On some OSs, connecting to localhost succeeds immediately. In
                // this case, queue the writable callback for execution during the
                // next event loop tick.
                debug!("socket connected immediately; addr={:?}", addr);
            }

            Ok(())
        }

        pub fn bind(self, addr: &SockAddr) -> MioResult<UnixListener> {
            try!(os::bind(&self.desc, addr));
            Ok(UnixListener { desc: self.desc })
        }
    }

    impl IoHandle for UnixSocket {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }
  
    impl FromIoDesc for UnixSocket {
        fn from_desc(desc: os::IoDesc) -> Self {
            UnixSocket { desc: desc }
        }
    }

    impl IoReader for UnixSocket {
        fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>> {
            io::read(self, buf)
        }

        fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
            io::read_slice(self, buf)
        }
    }

    impl IoWriter for UnixSocket {
        fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>> {
            io::write(self, buf)
        }

        fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
            io::write_slice(self, buf)
        }
    }

    impl Socket for UnixSocket {
    }

    #[derive(Debug)]
    pub struct UnixListener {
        desc: os::IoDesc,
    }

    impl UnixListener {
        pub fn listen(self, backlog: usize) -> MioResult<UnixAcceptor> {
            try!(os::listen(self.desc(), backlog));
            Ok(UnixAcceptor { desc: self.desc })
        }
    }

    impl IoHandle for UnixListener {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl FromIoDesc for UnixListener {
        fn from_desc(desc: os::IoDesc) -> Self {
            UnixListener { desc: desc }
        }
    }

    #[derive(Debug)]
    pub struct UnixAcceptor {
        desc: os::IoDesc,
    }

    impl UnixAcceptor {
        pub fn new(addr: &SockAddr, backlog: usize) -> MioResult<UnixAcceptor> {
            let sock = try!(UnixSocket::stream());
            let listener = try!(sock.bind(addr));
            listener.listen(backlog)
        }
    }

    impl IoHandle for UnixAcceptor {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl FromIoDesc for UnixAcceptor {
        fn from_desc(desc: os::IoDesc) -> Self {
            UnixAcceptor { desc: desc }
        }
    }

    impl Socket for UnixAcceptor {
    }

    impl IoAcceptor for UnixAcceptor {
        type Output = UnixSocket;

        fn accept(&mut self) -> MioResult<NonBlock<UnixSocket>> {
            match os::accept(self.desc()) {
                Ok(sock) => Ok(Ready(UnixSocket { desc: sock })),
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
