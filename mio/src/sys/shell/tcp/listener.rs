use super::TcpStream;
use crate::{event, Interest, Registry, Token};
use std::io;
use std::net::{self, SocketAddr};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

#[derive(Debug)]
pub struct TcpListener {}

impl TcpListener {
    pub fn bind(_: SocketAddr) -> io::Result<TcpListener> {
        os_required!();
    }

    pub fn from_std(_: net::TcpListener) -> TcpListener {
        os_required!()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        os_required!();
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        os_required!();
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        os_required!();
    }

    pub fn set_ttl(&self, _: u32) -> io::Result<()> {
        os_required!();
    }

    pub fn ttl(&self) -> io::Result<u32> {
        os_required!();
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        os_required!();
    }
}

impl event::Source for TcpListener {
    fn register(&mut self, _: &Registry, _: Token, _: Interest) -> io::Result<()> {
        os_required!();
    }

    fn reregister(&mut self, _: &Registry, _: Token, _: Interest) -> io::Result<()> {
        os_required!();
    }

    fn deregister(&mut self, _: &Registry) -> io::Result<()> {
        os_required!();
    }
}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        os_required!();
    }
}

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(_: RawFd) -> TcpListener {
        os_required!();
    }
}

impl IntoRawFd for TcpListener {
    fn into_raw_fd(self) -> RawFd {
        os_required!();
    }
}
