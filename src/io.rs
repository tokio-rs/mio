use std::fmt;
use std::path::Path;
use std::from_str::FromStr;
use buf::{Buf, MutBuf};
use os;
use error::MioResult;

// TODO: A lot of this will most likely get moved into OS specific files

pub use std::io::net::ip::{IpAddr, Port};
pub use std::io::net::ip::Ipv4Addr as IpV4Addr;

pub enum NonBlock<T> {
    Ready(T),
    WouldBlock
}

impl<T> NonBlock<T> {
    pub fn would_block(&self) -> bool {
        match *self {
            WouldBlock => true,
            _ => false
        }
    }

    pub fn unwrap(self) -> T {
        match self {
            Ready(v) => v,
            _ => fail!("would have blocked, no result to take")
        }
    }
}

pub trait IoHandle {
    fn desc(&self) -> &os::IoDesc;
}

// TODO: remove, not all IoDesc should implement both read and write (see
// pipe())
impl IoHandle for os::IoDesc {
    fn desc(&self) -> &os::IoDesc {
        self
    }
}

pub trait IoReader {
    fn read(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<()>>;
}

pub trait IoWriter {
    fn write(&mut self, buf: &mut Buf) -> MioResult<NonBlock<()>>;
}

pub trait IoAcceptor<T> {
    fn accept(&mut self) -> MioResult<NonBlock<T>>;
}

pub trait Socket : IoHandle {
    // Various sock opt fns
    fn is_acceptor(&self) -> bool {
        unimplemented!();
    }

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

impl<H: IoHandle> IoReader for H {
    fn read(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<()>> {
        let mut first_iter = true;

        while buf.has_remaining() {
            match os::read(self.desc(), buf.mut_bytes()) {
                // Successfully read some bytes, advance the cursor
                Ok(cnt) => {
                    buf.advance(cnt);
                    first_iter = false;
                }
                Err(e) => {
                    if e.is_would_block() {
                        return Ok(WouldBlock);
                    }

                    // If the EOF is hit the first time around, then propagate
                    if e.is_eof() {
                        if first_iter {
                            return Err(e);
                        }

                        return Ok(Ready(()));
                    }

                    // Indicate that the read was successful
                    return Err(e);
                }
            }
        }

        Ok(Ready(()))
    }
}

impl<H: IoHandle> IoWriter for H {
    fn write(&mut self, buf: &mut Buf) -> MioResult<NonBlock<()>> {
        while buf.has_remaining() {
            match os::write(self.desc(), buf.bytes()) {
                Ok(cnt) => buf.advance(cnt),
                Err(e) => {
                    if e.is_would_block() {
                        return Ok(WouldBlock);
                    }

                    return Err(e);
                }
            }
        }

        Ok(Ready(()))
    }
}

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

    pub fn bind(self, addr: &SockAddr) -> MioResult<TcpAcceptor> {
        try!(os::bind(&self.desc, addr))
        Ok(TcpAcceptor { desc: self.desc })
    }
}

impl IoHandle for TcpSocket {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
    }
}

impl Socket for TcpSocket {
}

#[deriving(Show)]
pub struct TcpAcceptor {
    desc: os::IoDesc
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

#[deriving(Show)]
pub struct PipeSender {
    desc: os::IoDesc
}

#[deriving(Show)]
pub struct PipeReceiver {
    desc: os::IoDesc
}

impl IoHandle for PipeSender {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
    }
}

impl IoHandle for PipeReceiver {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
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
