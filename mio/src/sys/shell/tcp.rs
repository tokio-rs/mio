#![allow(warnings)]

use crate::{event, Interests, Registry, Token};

use std::fmt;
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::{self, SocketAddr};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

#[derive(Debug)]
pub struct TcpListener {
}

impl TcpListener {
    pub fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
        os_required!();
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

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
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
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        os_required!();
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        os_required!();
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        os_required!();
    }
}

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpListener {
        os_required!();
    }
}

impl IntoRawFd for TcpListener {
    fn into_raw_fd(self) -> RawFd {
        os_required!();
    }
}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        os_required!();
    }
}

pub struct TcpStream {
}

impl TcpStream {
    pub(crate) fn new(inner: net::TcpStream) -> TcpStream {
        os_required!();
    }

    pub fn connect(addr: SocketAddr) -> io::Result<TcpStream> {
        os_required!();
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        os_required!();
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        os_required!();
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        os_required!();
    }

    pub fn shutdown(&self, how: net::Shutdown) -> io::Result<()> {
        os_required!();
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        os_required!();
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        os_required!();
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        os_required!();
    }

    pub fn ttl(&self) -> io::Result<u32> {
        os_required!();
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        os_required!();
    }

    pub fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        os_required!();
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        os_required!();
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        os_required!();
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        os_required!();
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        os_required!();
    }

    fn flush(&mut self) -> io::Result<()> {
        os_required!();
    }
}

impl event::Source for TcpStream {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        os_required!();
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        os_required!();
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        os_required!();
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        os_required!();
    }
}

impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpStream {
        os_required!();
    }
}

impl IntoRawFd for TcpStream {
    fn into_raw_fd(self) -> RawFd {
        os_required!();
    }
}

impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        os_required!();
    }
}
