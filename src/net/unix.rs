use {MioResult, MioError};
use buf::{Buf, MutBuf};
use io::{self, FromIoDesc, IoHandle, IoAcceptor, IoReader, IoWriter, NonBlock};
use net::{nix, Socket};
use os;
use std::path::Path;

#[derive(Debug)]
pub struct UnixSocket {
    desc: os::IoDesc
}

impl UnixSocket {
    pub fn stream() -> MioResult<UnixSocket> {
        UnixSocket::new(nix::SockType::Stream)
    }

    fn new(ty: nix::SockType) -> MioResult<UnixSocket> {
        Ok(UnixSocket {
            desc: try!(os::socket(nix::AddressFamily::Unix, ty))
        })
    }

    pub fn connect(&self, addr: &Path) -> MioResult<bool> {
        // Attempt establishing the context. This may not complete immediately.
        os::connect(&self.desc, &try!(to_nix_addr(addr)))
    }

    pub fn bind(self, addr: &Path) -> MioResult<UnixListener> {
        try!(os::bind(&self.desc, &try!(to_nix_addr(addr))));
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
    pub fn new(addr: &Path, backlog: usize) -> MioResult<UnixAcceptor> {
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
            Ok(sock) => Ok(NonBlock::Ready(UnixSocket { desc: sock })),
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
