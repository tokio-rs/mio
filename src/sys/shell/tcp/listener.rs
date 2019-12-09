use super::TcpStream;
use crate::{event, Interest, Registry, Token};
use std::io;
use std::net::{self, SocketAddr};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};

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

#[cfg(unix)]
impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        os_required!();
    }
}

#[cfg(unix)]
impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(_: RawFd) -> TcpListener {
        os_required!();
    }
}

#[cfg(unix)]
impl IntoRawFd for TcpListener {
    fn into_raw_fd(self) -> RawFd {
        os_required!();
    }
}

#[cfg(windows)]
impl AsRawSocket for TcpListener {
    fn as_raw_socket(&self) -> RawSocket {
        os_required!()
    }
}

#[cfg(windows)]
impl FromRawSocket for TcpListener {
    unsafe fn from_raw_socket(_: RawSocket) -> TcpListener {
        os_required!()
    }
}

#[cfg(windows)]
impl IntoRawSocket for TcpListener {
    fn into_raw_socket(self) -> RawSocket {
        os_required!()
    }
}
