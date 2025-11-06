use std::fmt::Debug;
use std::io;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;

use crate::event;

use super::Socket;
use super::SocketAddr;
#[derive(Debug)]
pub struct UnixStream(pub Socket);
impl UnixStream {}
impl io::Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut &*self, buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut &*self)
    }
}
impl io::Write for &UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl io::Read for &UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buf)
    }
}
impl io::Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut &*self, buf)
    }
}
impl Deref for UnixStream {
    type Target = Socket;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for UnixStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
pub fn connect(path: &Path) -> io::Result<UnixStream> {
    todo!()
}
pub fn connect_addr(address: &SocketAddr) -> io::Result<UnixStream> {
    todo!()
}
pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
    todo!()
}
impl event::Source for UnixStream {
    fn register(
        &mut self,
        registry: &crate::Registry,
        token: crate::Token,
        interests: crate::Interest,
    ) -> io::Result<()> {
        todo!()
    }

    fn reregister(
        &mut self,
        registry: &crate::Registry,
        token: crate::Token,
        interests: crate::Interest,
    ) -> io::Result<()> {
        todo!()
    }

    fn deregister(&mut self, registry: &crate::Registry) -> io::Result<()> {
        todo!()
    }
}
