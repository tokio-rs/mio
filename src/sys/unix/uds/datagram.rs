use super::{pair_descriptors, socket_addr};
use crate::event::Source;
use crate::sys::unix::net::new_socket;
use crate::unix::SourceFd;
use crate::{Interests, Registry, Token};

use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;

#[derive(Debug)]
pub struct UnixDatagram {
    inner: net::UnixDatagram,
}

impl UnixDatagram {
    fn new(inner: net::UnixDatagram) -> UnixDatagram {
        UnixDatagram { inner }
    }

    pub(crate) fn bind(path: &Path) -> io::Result<UnixDatagram> {
        let socket = new_socket(libc::AF_UNIX, libc::SOCK_DGRAM)?;
        let (sockaddr, socklen) = socket_addr(path)?;
        let sockaddr = &sockaddr as *const libc::sockaddr_un as *const libc::sockaddr;

        syscall!(bind(socket, sockaddr, socklen))?;
        Ok(unsafe { UnixDatagram::from_raw_fd(socket) })
    }

    pub(crate) fn connect<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self.inner.connect(path)
    }

    pub(crate) fn pair() -> io::Result<(UnixDatagram, UnixDatagram)> {
        let fds = [-1; 2];
        let flags = libc::SOCK_DGRAM;

        pair_descriptors(fds, flags)?;

        Ok(unsafe {
            (
                UnixDatagram::from_raw_fd(fds[0]),
                UnixDatagram::from_raw_fd(fds[1]),
            )
        })
    }

    pub(crate) fn unbound() -> io::Result<UnixDatagram> {
        let socket = new_socket(libc::AF_UNIX, libc::SOCK_DGRAM)?;
        Ok(unsafe { UnixDatagram::from_raw_fd(socket) })
    }

    pub(crate) fn try_clone(&self) -> io::Result<UnixDatagram> {
        let inner = self.inner.try_clone()?;
        Ok(UnixDatagram::new(inner))
    }

    pub(crate) fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub(crate) fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    pub(crate) fn recv_from(&self, dst: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(dst)
    }

    pub(crate) fn recv(&self, dst: &mut [u8]) -> io::Result<usize> {
        self.inner.recv(dst)
    }

    pub(crate) fn send_to<P: AsRef<Path>>(&self, src: &[u8], path: P) -> io::Result<usize> {
        self.inner.send_to(src, path)
    }

    pub(crate) fn send(&self, src: &[u8]) -> io::Result<usize> {
        self.inner.send(src)
    }

    pub(crate) fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }

    pub(crate) fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }
}

impl Source for UnixDatagram {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).deregister(registry)
    }
}

impl AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl FromRawFd for UnixDatagram {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixDatagram {
        UnixDatagram::new(net::UnixDatagram::from_raw_fd(fd))
    }
}

impl IntoRawFd for UnixDatagram {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}
