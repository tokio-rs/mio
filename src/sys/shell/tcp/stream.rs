use crate::{event, Interest, Registry, Token};
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::{self, SocketAddr};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};

#[derive(Debug)]
pub struct TcpStream {}

impl TcpStream {
    pub fn new(_: net::TcpStream) -> TcpStream {
        os_required!();
    }

    pub fn connect(_: SocketAddr) -> io::Result<TcpStream> {
        os_required!();
    }

    pub fn from_std(_: net::TcpStream) -> TcpStream {
        os_required!();
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        os_required!();
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        os_required!();
    }

    pub fn shutdown(&self, _: net::Shutdown) -> io::Result<()> {
        os_required!();
    }

    pub fn set_nodelay(&self, _: bool) -> io::Result<()> {
        os_required!();
    }

    pub fn nodelay(&self) -> io::Result<bool> {
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

    pub fn peek(&self, _: &mut [u8]) -> io::Result<usize> {
        os_required!();
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        os_required!();
    }

    fn read_vectored(&mut self, _: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        os_required!();
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        os_required!();
    }

    fn write_vectored(&mut self, _: &[IoSlice<'_>]) -> io::Result<usize> {
        os_required!();
    }

    fn flush(&mut self) -> io::Result<()> {
        os_required!();
    }
}

impl event::Source for TcpStream {
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
impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        os_required!();
    }
}

#[cfg(unix)]
impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(_: RawFd) -> TcpStream {
        os_required!();
    }
}

#[cfg(unix)]
impl IntoRawFd for TcpStream {
    fn into_raw_fd(self) -> RawFd {
        os_required!();
    }
}

#[cfg(windows)]
impl AsRawSocket for TcpStream {
    fn as_raw_socket(&self) -> RawSocket {
        os_required!()
    }
}

#[cfg(windows)]
impl FromRawSocket for TcpStream {
    unsafe fn from_raw_socket(_: RawSocket) -> TcpStream {
        os_required!()
    }
}

#[cfg(windows)]
impl IntoRawSocket for TcpStream {
    fn into_raw_socket(self) -> RawSocket {
        os_required!()
    }
}
