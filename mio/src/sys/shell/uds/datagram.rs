use crate::{event, sys, Interest, Registry, Token};
use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net;
use std::path::Path;

#[derive(Debug)]
pub struct UnixDatagram {}

impl UnixDatagram {
    pub fn bind<P: AsRef<Path>>(_: P) -> io::Result<UnixDatagram> {
        os_required!()
    }

    pub fn from_std(_: net::UnixDatagram) -> UnixDatagram {
        os_required!()
    }

    pub fn connect<P: AsRef<Path>>(&self, _: P) -> io::Result<()> {
        os_required!()
    }

    pub fn unbound() -> io::Result<UnixDatagram> {
        os_required!()
    }

    pub fn pair() -> io::Result<(UnixDatagram, UnixDatagram)> {
        os_required!()
    }

    pub fn try_clone(&self) -> io::Result<UnixDatagram> {
        os_required!()
    }

    pub fn local_addr(&self) -> io::Result<sys::SocketAddr> {
        os_required!()
    }

    pub fn peer_addr(&self) -> io::Result<sys::SocketAddr> {
        os_required!()
    }

    pub fn recv_from(&self, _: &mut [u8]) -> io::Result<(usize, sys::SocketAddr)> {
        os_required!()
    }

    pub fn recv(&self, _: &mut [u8]) -> io::Result<usize> {
        os_required!()
    }

    pub fn send_to<P: AsRef<Path>>(&self, _: &[u8], _: P) -> io::Result<usize> {
        os_required!()
    }

    pub fn send(&self, _: &[u8]) -> io::Result<usize> {
        os_required!()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        os_required!()
    }

    pub fn shutdown(&self, _: Shutdown) -> io::Result<()> {
        os_required!()
    }
}

impl event::Source for UnixDatagram {
    fn register(&mut self, _: &Registry, _: Token, _: Interest) -> io::Result<()> {
        os_required!()
    }

    fn reregister(&mut self, _: &Registry, _: Token, _: Interest) -> io::Result<()> {
        os_required!()
    }

    fn deregister(&mut self, _: &Registry) -> io::Result<()> {
        os_required!()
    }
}

impl AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> RawFd {
        os_required!()
    }
}

impl FromRawFd for UnixDatagram {
    unsafe fn from_raw_fd(_: RawFd) -> UnixDatagram {
        os_required!()
    }
}

impl IntoRawFd for UnixDatagram {
    fn into_raw_fd(self) -> RawFd {
        os_required!()
    }
}
