use std::fmt;
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::mem::size_of;
use std::net::{self, SocketAddr};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

use crate::sys::unix::net::{new_socket, socket_addr};
use crate::sys::unix::SourceFd;
use crate::{event, Interests, Registry, Token};

pub struct TcpStream {
    inner: net::TcpStream,
}

pub struct TcpListener {
    inner: net::TcpListener,
}

impl TcpStream {
    pub fn connect(addr: SocketAddr) -> io::Result<TcpStream> {
        new_socket(addr, libc::SOCK_STREAM)
            .and_then(|socket| {
                let (raw_addr, raw_addr_length) = socket_addr(&addr);
                syscall!(connect(socket, raw_addr, raw_addr_length))
                    .or_else(|err| match err {
                        // Connect hasn't finished, but that is fine.
                        ref err if err.raw_os_error() == Some(libc::EINPROGRESS) => Ok(0),
                        err => Err(err),
                    })
                    .map(|_| socket)
                    .map_err(|err| {
                        // Close the socket if we hit an error, ignoring the error
                        // from closing since we can't pass back two errors.
                        let _ = unsafe { libc::close(socket) };
                        err
                    })
            })
            .map(|socket| TcpStream {
                inner: unsafe { net::TcpStream::from_raw_fd(socket) },
            })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.inner.try_clone().map(|s| TcpStream { inner: s })
    }

    pub fn shutdown(&self, how: net::Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.inner.set_nodelay(nodelay)
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        self.inner.nodelay()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.inner.set_ttl(ttl)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        self.inner.ttl()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }

    pub fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.peek(buf)
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.inner).read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        (&self.inner).read_vectored(bufs)
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.inner).write(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        (&self.inner).write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.inner).flush()
    }
}

impl event::Source for TcpStream {
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

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpStream {
        TcpStream {
            inner: net::TcpStream::from_raw_fd(fd),
        }
    }
}

impl IntoRawFd for TcpStream {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl TcpListener {
    pub fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
        new_socket(addr, libc::SOCK_STREAM).and_then(|socket| {
            // Set SO_REUSEADDR (mirrors what libstd does).
            syscall!(setsockopt(
                socket,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &1 as *const libc::c_int as *const libc::c_void,
                size_of::<libc::c_int>() as libc::socklen_t,
            ))
            .and_then(|_| {
                let (raw_addr, raw_addr_length) = socket_addr(&addr);
                syscall!(bind(socket, raw_addr, raw_addr_length))
            })
            .and_then(|_| syscall!(listen(socket, 1024)))
            .map_err(|err| {
                // Close the socket if we hit an error, ignoring the error
                // from closing since we can't pass back two errors.
                let _ = unsafe { libc::close(socket) };
                err
            })
            .map(|_| TcpListener {
                inner: unsafe { net::TcpListener::from_raw_fd(socket) },
            })
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.inner.try_clone().map(|s| TcpListener { inner: s })
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        self.inner
            .accept()
            .map(|(inner, addr)| (TcpStream { inner }, addr))
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.inner.set_ttl(ttl)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        self.inner.ttl()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }
}

impl event::Source for TcpListener {
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

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpListener {
        TcpListener {
            inner: net::TcpListener::from_raw_fd(fd),
        }
    }
}

impl IntoRawFd for TcpListener {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
