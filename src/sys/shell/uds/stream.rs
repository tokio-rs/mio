use crate::{event, sys, Interest, Registry, Token};
use std::io::{self, IoSlice, IoSliceMut};
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net;
use std::path::Path;

#[derive(Debug)]
pub struct UnixStream {}

impl UnixStream {
    pub fn connect<P: AsRef<Path>>(_: P) -> io::Result<UnixStream> {
        os_required!()
    }

    pub fn from_std(_: net::UnixStream) -> UnixStream {
        os_required!()
    }

    pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
        os_required!()
    }

    pub fn local_addr(&self) -> io::Result<sys::SocketAddr> {
        os_required!()
    }

    pub fn peer_addr(&self) -> io::Result<sys::SocketAddr> {
        os_required!()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        os_required!()
    }

    pub fn shutdown(&self, _: Shutdown) -> io::Result<()> {
        os_required!()
    }
}

impl event::Source for UnixStream {
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

impl io::Read for UnixStream {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        os_required!()
    }

    fn read_vectored(&mut self, _: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        os_required!()
    }
}

impl<'a> io::Read for &'a UnixStream {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        os_required!()
    }

    fn read_vectored(&mut self, _: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        os_required!()
    }
}

impl io::Write for UnixStream {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        os_required!()
    }

    fn write_vectored(&mut self, _: &[IoSlice<'_>]) -> io::Result<usize> {
        os_required!()
    }

    fn flush(&mut self) -> io::Result<()> {
        os_required!()
    }
}

impl<'a> io::Write for &'a UnixStream {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        os_required!()
    }

    fn write_vectored(&mut self, _: &[IoSlice<'_>]) -> io::Result<usize> {
        os_required!()
    }

    fn flush(&mut self) -> io::Result<()> {
        os_required!()
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        os_required!()
    }
}

impl FromRawFd for UnixStream {
    unsafe fn from_raw_fd(_: RawFd) -> UnixStream {
        os_required!()
    }
}

impl IntoRawFd for UnixStream {
    fn into_raw_fd(self) -> RawFd {
        os_required!()
    }
}
