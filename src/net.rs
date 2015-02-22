//! Networking primitives
//!
use os;
use io::{Listenable, NonBlock};
use buf::{Buf, MutBuf};
use std::fmt;
use std::io::Result;
use std::str::FromStr;
use std::os::unix::Fd;

pub use os::{InetAddr, IpAddr, SocketType};
pub use std::net::{Ipv4Addr, Ipv6Addr};

pub trait Socket : Listenable {

    fn linger(&self) -> Result<usize> {
        os::linger(self.as_fd())
    }

    fn set_linger(&self, dur_s: usize) -> Result<()> {
        os::set_linger(self.as_fd(), dur_s)
    }

    fn set_reuseaddr(&self, val: bool) -> Result<()> {
        os::set_reuseaddr(self.as_fd(), val)
    }

    fn set_reuseport(&self, val: bool) -> Result<()> {
        os::set_reuseport(self.as_fd(), val)
    }
}

pub trait MulticastSocket : Socket {

    fn join_multicast_group(&self, addr: &IpAddr, interface: &Option<IpAddr>) -> Result<()> {
        os::join_multicast_group(self.as_fd(), addr, interface)
    }

    fn leave_multicast_group(&self, addr: &IpAddr, interface: &Option<IpAddr>) -> Result<()> {
        os::leave_multicast_group(self.as_fd(), addr, interface)
    }

    fn set_multicast_ttl(&self, val: u8) -> Result<()> {
        os::set_multicast_ttl(self.as_fd(), val)
    }
}

pub trait UnconnectedSocket {

    fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &InetAddr) -> Result<NonBlock<()>>;

    fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> Result<NonBlock<InetAddr>>;
}

/// TCP networking primitives
///
pub mod tcp {
    use buf::{Buf, MutBuf};
    use io::{self, NonBlock};
    use os::{self, AddressFamily};
    use net::{Socket, InetAddr};
    use net::SocketType::Stream;
    use std::io::Result;
    use std::os::unix::Fd;

    #[derive(Debug)]
    pub struct TcpSocket {
        fd: Fd,
    }

    impl TcpSocket {
        pub fn v4() -> Result<TcpSocket> {
            TcpSocket::new(AddressFamily::Inet)
        }

        pub fn v6() -> Result<TcpSocket> {
            TcpSocket::new(AddressFamily::Inet6)
        }

        fn new(family: AddressFamily) -> Result<TcpSocket> {
            os::socket(family, Stream)
                .map(|fd| TcpSocket { fd: fd })
        }

        /// Open a TCP connection to a remote host.
        pub fn connect(&self, addr: &InetAddr) -> Result<NonBlock<()>> {
            // TODO: return a TcpListner and signal the blocking-ness
            //
            if try!(os::connect(self.fd, addr)) {
                Ok(NonBlock::Ready(()))
            } else {
                Ok(NonBlock::WouldBlock)
            }
        }

        /// Assign the specified address to the socket
        ///
        /// [Further reading](http://man7.org/linux/man-pages/man2/bind.2.html)
        pub fn bind(&self, addr: &InetAddr) -> Result<()> {
            os::bind(self.fd, addr)
        }
    }

    impl Socket for TcpSocket {
    }
}

pub mod udp {
    use {io, TryRead, TryWrite, NonBlock};
    use os::{self, AddressFamily, SocketType};
    use bytes::{Buf, MutBuf};
    use net::{Socket, MulticastSocket, InetAddr, UnconnectedSocket};
    use net::SocketType::Dgram;
    use std::os::unix::Fd;

    #[derive(Debug)]
    pub struct UdpSocket {
        fd: Fd,
    }

    impl UdpSocket {
        pub fn v4() -> Result<UdpSocket> {
            UdpSocket::new(AddressFamily::Inet)
        }

        fn new(family: AddressFamily) -> Result<UdpSocket> {
            os::socket(family, SocketType::Dgram)
                .map(|fd| UdpSocket { fd: fd })
        }

        pub fn bind(&self, addr: &InetAddr) -> Result<()> {
            os::bind(self.fd, addr)
        }

        pub fn connect(&self, addr: &InetAddr) -> Result<bool> {
            os::connect(self.fd, addr)
        }

        pub fn bound(addr: &InetAddr) -> Result<UdpSocket> {
            let sock = try!(UdpSocket::new(addr.family()));
            try!(sock.bind(addr));
            Ok(sock)
        }
    }

    impl Socket for UdpSocket {
    }

    impl MulticastSocket for UdpSocket {
    }

    // Unconnected socket sender -- trait unique to sockets
    impl UnconnectedSocket for UdpSocket {
        fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &InetAddr) -> Result<NonBlock<()>> {
            match os::sendto(&self.desc, buf.bytes(), tgt) {
                Ok(cnt) => {
                    buf.advance(cnt);
                    Ok(NonBlock::Ready(()))
                }
                Err(e) => {
                    if e.is_would_block() {
                        Ok(NonBlock::WouldBlock)
                    } else {
                        Err(e)
                    }
                }
            }
        }

        fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> Result<NonBlock<InetAddr>> {
            match os::recvfrom(&self.desc, buf.mut_bytes()) {
                Ok((cnt, saddr)) => {
                    buf.advance(cnt);
                    Ok(NonBlock::Ready(saddr))
                }
                Err(e) => {
                    if e.is_would_block() {
                        Ok(NonBlock::WouldBlock)
                    } else {
                        Err(e)
                    }
                }
            }
        }
    }
}

/// Unix domain sockets
pub mod unix {
    use {io, TryRead, TryWrite, NonBlock};
    use os::{self, AddressFamily};
    use buf::{Buf, MutBuf};
    use net::{Socket, SocketType};
    use std::os::unix::Fd;

    #[derive(Debug)]
    pub struct UnixSocket {
        fd: Fd,
    }

    impl UnixSocket {
        pub fn stream() -> Result<UnixSocket> {
            UnixSocket::new(SocketType::Stream)
        }

        pub fn datagram() -> Result<UnixSocket> {
            UnixSocket::new(SocketType::Dgram)
        }

        fn new(socktype: SocketType) -> Result<UnixSocket> {
            os::socket(AddressFamily::Unix, socktype)
                .map(|fd| UnixSocket { fd: fd })
        }

        pub fn connect(&self, path: &Path) -> Result<()> {
            // TODO: return a UnixListener and signal the blocking-ness
            //
            if try!(os::connect(self.fd, path)) {
                Ok(NonBlock::Ready(()))
            } else {
                Ok(NonBlock::WouldBlock)
            }
        }

        pub fn bind(self, path: &Path) -> Result<UnixListener> {
            try!(os::bind(&self.desc, path));
            Ok(UnixListener { desc: self.desc })
        }
    }

    impl Socket for UnixSocket {
    }

    #[derive(Debug)]
    pub struct UnixListener {
        fd: Fd,
    }

    impl UnixListener {
        pub fn listen(self, backlog: usize) -> Result<UnixAcceptor> {
            try!(os::listen(self.desc(), backlog));
            Ok(UnixAcceptor { desc: self.desc })
        }
    }

    #[derive(Debug)]
    pub struct UnixAcceptor {
        fd: Fd,
    }

    impl UnixAcceptor {
        pub fn new(path: &Path, backlog: usize) -> Result<UnixAcceptor> {
            let sock = try!(UnixSocket::stream());
            let listener = try!(sock.bind(path));
            listener.listen(backlog)
        }

        pub fn accept(&mut self) -> Result<NonBlock<UnixSocket>> {
            os::accept(self.fd)
                .map(|fd| Ok(NonBlock::Ready(UnixSocket { fd: fd })))
                .or_else(io::to_non_block_res)
        }
    }

    impl Socket for UnixAcceptor {
    }
}
