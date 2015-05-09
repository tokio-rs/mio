use {io, Evented, Interest, Io, PollOpt, Selector, Token, TryRead, TryWrite};
use unix::FromRawFd;
use sys::unix::{net, nix, Socket};
use std::path::Path;
use std::os::unix::io::{RawFd, AsRawFd};

#[derive(Debug)]
pub struct UnixSocket {
    io: Io,
}

impl UnixSocket {
    /// Returns a new, unbound, non-blocking Unix domain socket
    pub fn stream() -> io::Result<UnixSocket> {
        UnixSocket::new(nix::SockType::Stream)
    }

    fn new(ty: nix::SockType) -> io::Result<UnixSocket> {
        let fd = try!(net::socket(nix::AddressFamily::Unix, ty, true));
        Ok(From::from(Io::from_raw_fd(fd)))
    }

    /// Connect the socket to the specified address
    pub fn connect<P: AsRef<Path> + ?Sized>(&self, addr: &P) -> io::Result<bool> {
        net::connect(&self.io, &try!(to_nix_addr(addr)))
    }

    /// Listen for incoming requests
    pub fn listen(&self, backlog: usize) -> io::Result<()> {
        net::listen(&self.io, backlog)
    }

    pub fn accept(&self) -> io::Result<Option<UnixSocket>> {
        net::accept(&self.io, true)
            .map(|fd| Some(From::from(Io::from_raw_fd(fd))))
            .or_else(io::to_non_block)
    }

    /// Bind the socket to the specified address
    pub fn bind<P: AsRef<Path> + ?Sized>(&self, addr: &P) -> io::Result<()> {
        net::bind(&self.io, &try!(to_nix_addr(addr)))
    }

    pub fn try_clone(&self) -> io::Result<UnixSocket> {
        unimplemented!();
    }
}

impl TryRead for UnixSocket {
    fn read_slice(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        self.io.read_slice(buf)
    }
}

impl TryWrite for UnixSocket {
    fn write_slice(&mut self, buf: &[u8]) -> io::Result<Option<usize>> {
        self.io.write_slice(buf)
    }
}

impl Evented for UnixSocket {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.io.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.io.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.io.deregister(selector)
    }
}

impl Socket for UnixSocket {
}

impl From<Io> for UnixSocket {
    fn from(io: Io) -> UnixSocket {
        UnixSocket { io: io }
    }
}

impl FromRawFd for UnixSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixSocket {
        UnixSocket { io: Io::from_raw_fd(fd) }
    }
}

impl AsRawFd for UnixSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

fn to_nix_addr<P: AsRef<Path> + ?Sized>(path: &P) -> io::Result<nix::SockAddr> {
    nix::SockAddr::new_unix(path.as_ref())
        .map_err(super::from_nix_error)
}
