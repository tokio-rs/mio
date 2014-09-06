use std::fmt;
use std::path::Path;
use std::from_str::FromStr;
use buf::{Buf, MutBuf};
use os;
use error::MioResult;

// TODO: A lot of this will most likely get moved into OS specific files

pub use std::io::net::ip::{IpAddr, Port};
pub use std::io::net::ip::Ipv4Addr as IpV4Addr;

pub trait IoHandle {
    fn desc(&self) -> os::IoDesc;
}

impl IoHandle for os::IoDesc {
    fn desc(&self) -> os::IoDesc {
        *self
    }
}

// TODO: Should read / write return bool to indicate whether or not there is more?
pub trait IoReader {
    fn read(&mut self, buf: &mut MutBuf) -> MioResult<()>;
}

pub trait IoWriter {
    fn write(&mut self, buf: &mut Buf) -> MioResult<()>;
}

pub trait IoAcceptor<T> {
    fn accept(&mut self) -> MioResult<T>;
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
    fn read(&mut self, buf: &mut MutBuf) -> MioResult<()> {
        while !buf.is_full() {
            match os::read(self.desc(), buf.mut_bytes()) {
                Ok(cnt) => buf.advance(cnt),
                Err(e) if e.is_eof() => return Ok(()),
                Err(e) if e.is_would_block() => return Ok(()),
                Err(e) => return Err(e)
            }
        }

        Ok(())
    }
}

impl<H: IoHandle> IoWriter for H {
    fn write(&mut self, buf: &mut Buf) -> MioResult<()> {
        while !buf.is_full() {
            match os::write(self.desc(), buf.bytes()) {
                Ok(cnt) => buf.advance(cnt),
                Err(e) if e.is_eof() => return Ok(()),
                Err(e) if e.is_would_block() => return Ok(()),
                Err(e) => return Err(e)
            }
        }

        Ok(())
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

#[deriving(Show)]
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

#[deriving(Show)]
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

#[deriving(Show)]
pub struct PipeSender {
    desc: os::IoDesc
}

#[deriving(Show)]
pub struct PipeReceiver {
    desc: os::IoDesc
}

impl IoHandle for PipeSender {
    fn desc(&self) -> os::IoDesc {
        self.desc
    }
}

impl IoHandle for PipeReceiver {
    fn desc(&self) -> os::IoDesc {
        self.desc
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
