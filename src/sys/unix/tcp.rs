use std::fmt;
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::mem::size_of_val;
use std::net::{self, SocketAddr};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::time::Duration;

use libc;
use net2::TcpStreamExt;

use crate::sys::unix::io::set_nonblock;
use crate::sys::unix::SourceFd;
use crate::{event, Interests, Registry, Token};

pub struct TcpStream {
    inner: net::TcpStream,
}

pub struct TcpListener {
    inner: net::TcpListener,
}

impl TcpStream {
    pub fn connect(address: SocketAddr) -> io::Result<TcpStream> {
        let domain = match address {
            SocketAddr::V4(..) => libc::AF_INET,
            SocketAddr::V6(..) => libc::AF_INET6,
        };
        #[cfg(any(
            target_os = "ios", // Darwin doesn't have SOCK_NONBLOCK or SOCK_CLOEXEC.
            target_os = "macos",
            target_os = "solaris" // Not sure about Solaris, couldn't find anything online.
        ))]
        let socket_type = libc::SOCK_STREAM;
        #[cfg(any(
            target_os = "android",
            target_os = "bitrig",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "linux",
            target_os = "netbsd",
            target_os = "openbsd"
        ))]
        let socket_type = libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

        let socket = syscall!(socket(domain, socket_type, 0));

        #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
        let socket = socket.and_then(|socket| {
            // For platforms that don't support flags in socket, we need to
            // set the flags ourselves.
            syscall!(fcntl(
                socket,
                libc::F_SETFL,
                libc::O_NONBLOCK | libc::O_CLOEXEC
            ))
            .map(|_| socket)
        });

        socket
            .and_then(|socket| {
                let (raw_address, raw_address_length) = socket_address(&address);
                syscall!(connect(socket, raw_address, raw_address_length))
                    .or_else(|err| match err {
                        // Connect hasn't finished, but that is fine.
                        ref err if err.raw_os_error() == Some(libc::EINPROGRESS) => Ok(0),
                        err => Err(err),
                    })
                    .map(|_| socket)
            })
            .map(|socket| TcpStream {
                inner: unsafe { net::TcpStream::from_raw_fd(socket) },
            })
    }

    pub fn connect_stream(stream: net::TcpStream, addr: SocketAddr) -> io::Result<TcpStream> {
        set_nonblock(stream.as_raw_fd())?;

        match stream.connect(addr) {
            Ok(..) => {}
            Err(ref e) if e.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(e) => return Err(e),
        }

        Ok(TcpStream { inner: stream })
    }

    pub fn from_stream(stream: net::TcpStream) -> TcpStream {
        TcpStream { inner: stream }
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

    pub fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
        self.inner.set_recv_buffer_size(size)
    }

    pub fn recv_buffer_size(&self) -> io::Result<usize> {
        self.inner.recv_buffer_size()
    }

    pub fn set_send_buffer_size(&self, size: usize) -> io::Result<()> {
        self.inner.set_send_buffer_size(size)
    }

    pub fn send_buffer_size(&self) -> io::Result<usize> {
        self.inner.send_buffer_size()
    }

    pub fn set_keepalive(&self, keepalive: Option<Duration>) -> io::Result<()> {
        self.inner.set_keepalive(keepalive)
    }

    pub fn keepalive(&self) -> io::Result<Option<Duration>> {
        self.inner.keepalive()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.inner.set_ttl(ttl)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        self.inner.ttl()
    }

    pub fn set_linger(&self, dur: Option<Duration>) -> io::Result<()> {
        self.inner.set_linger(dur)
    }

    pub fn linger(&self) -> io::Result<Option<Duration>> {
        self.inner.linger()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }

    pub fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.peek(buf)
    }
}

fn socket_address(address: &SocketAddr) -> (*const libc::sockaddr, libc::socklen_t) {
    match address {
        SocketAddr::V4(ref address) => (
            address as *const _ as *const libc::sockaddr,
            size_of_val(address) as libc::socklen_t,
        ),
        SocketAddr::V6(ref address) => (
            address as *const _ as *const libc::sockaddr,
            size_of_val(address) as libc::socklen_t,
        ),
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
    pub fn new(inner: net::TcpListener) -> io::Result<TcpListener> {
        set_nonblock(inner.as_raw_fd())?;
        Ok(TcpListener { inner })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.inner.try_clone().map(|s| TcpListener { inner: s })
    }

    pub fn accept(&self) -> io::Result<(net::TcpStream, SocketAddr)> {
        self.inner.accept()
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
