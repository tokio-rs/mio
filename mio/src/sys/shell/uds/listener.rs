use super::UnixStream;
use crate::unix::SocketAddr;
use crate::{event, sys, Interest, Registry, Token};
use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net;
use std::path::Path;

#[derive(Debug)]
pub struct UnixListener {}

impl UnixListener {
    pub fn bind<P: AsRef<Path>>(_: P) -> io::Result<UnixListener> {
        os_required!()
    }

    pub fn from_std(_: net::UnixListener) -> UnixListener {
        os_required!()
    }

    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        os_required!()
    }

    pub fn try_clone(&self) -> io::Result<UnixListener> {
        os_required!()
    }

    pub fn local_addr(&self) -> io::Result<sys::SocketAddr> {
        os_required!()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        os_required!()
    }
}

impl event::Source for UnixListener {
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

#[cfg(unix)]
impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        os_required!()
    }
}

#[cfg(unix)]
impl FromRawFd for UnixListener {
    unsafe fn from_raw_fd(_: RawFd) -> UnixListener {
        os_required!()
    }
}

#[cfg(unix)]
impl IntoRawFd for UnixListener {
    fn into_raw_fd(self) -> RawFd {
        os_required!()
    }
}
