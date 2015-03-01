use {TryRead, TryWrite, NonBlock, MioResult, MioError};
use buf::{Buf, MutBuf};
use io::{Io, FromFd, IoHandle, IoAcceptor};
use net::{self, nix, Socket};
use std::path::Path;
use std::os::unix::Fd;

#[derive(Debug)]
pub struct UnixSocket {
    io: Io,
}

impl UnixSocket {
    pub fn stream() -> MioResult<UnixSocket> {
        UnixSocket::new(nix::SockType::Stream)
    }

    fn new(ty: nix::SockType) -> MioResult<UnixSocket> {
        let fd = try!(net::socket(nix::AddressFamily::Unix, ty));
        Ok(FromFd::from_fd(fd))
    }

    pub fn connect(&self, addr: &Path) -> MioResult<bool> {
        // Attempt establishing the context. This may not complete immediately.
        net::connect(&self.io, &try!(to_nix_addr(addr)))
    }

    pub fn bind(self, addr: &Path) -> MioResult<UnixListener> {
        try!(net::bind(&self.io, &try!(to_nix_addr(addr))));
        Ok(UnixListener { io: self.io })
    }
}

impl IoHandle for UnixSocket {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
}

impl FromFd for UnixSocket {
    fn from_fd(fd: Fd) -> UnixSocket {
        UnixSocket { io: Io::new(fd) }
    }
}

impl TryRead for UnixSocket {
    fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
        self.io.read_slice(buf)
    }
}

impl TryWrite for UnixSocket {
    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
        self.io.write_slice(buf)
    }
}

impl Socket for UnixSocket {
}

#[derive(Debug)]
pub struct UnixListener {
    io: Io,
}

impl UnixListener {
    pub fn listen(self, backlog: usize) -> MioResult<UnixAcceptor> {
        try!(net::listen(&self.io, backlog));
        Ok(UnixAcceptor { io: self.io })
    }
}

impl IoHandle for UnixListener {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
}

impl FromFd for UnixListener {
    fn from_fd(fd: Fd) -> UnixListener {
        UnixListener { io: Io::new(fd) }
    }
}

#[derive(Debug)]
pub struct UnixAcceptor {
    io: Io,
}

impl UnixAcceptor {
    pub fn new(addr: &Path, backlog: usize) -> MioResult<UnixAcceptor> {
        let sock = try!(UnixSocket::stream());
        let listener = try!(sock.bind(addr));
        listener.listen(backlog)
    }
}

impl IoHandle for UnixAcceptor {
    fn fd(&self) -> Fd {
        self.io.fd()
    }
}

impl FromFd for UnixAcceptor {
    fn from_fd(fd: Fd) -> UnixAcceptor {
        UnixAcceptor { io: Io::new(fd) }
    }
}

impl Socket for UnixAcceptor {
}

impl IoAcceptor for UnixAcceptor {
    type Output = UnixSocket;

    fn accept(&mut self) -> MioResult<NonBlock<UnixSocket>> {
        match net::accept(&self.io) {
            Ok(fd) => Ok(NonBlock::Ready(FromFd::from_fd(fd))),
            Err(e) => {
                if e.is_would_block() {
                    return Ok(NonBlock::WouldBlock);
                }

                return Err(e);
            }
        }
    }
}

fn to_nix_addr(path: &Path) -> MioResult<nix::SockAddr> {
    nix::SockAddr::new_unix(path)
        .map_err(MioError::from_nix_error)
}
