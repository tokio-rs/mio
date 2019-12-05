use super::{inaddr_any, new_socket, socket_addr};
use crate::sys::windows::init;
use crate::windows::{SocketState, SourceSocket};
use crate::{event, Interest, Registry, Token};

use std::fmt;
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::{self, SocketAddr};
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::os::windows::raw::SOCKET as StdSocket; // winapi uses usize, stdlib uses u32/u64.
use winapi::um::winsock2::{bind, closesocket, connect, listen, SOCKET_ERROR, SOCK_STREAM};

pub struct TcpStream {
    state: SocketState,
    inner: net::TcpStream,
}

impl TcpStream {
    pub fn connect(addr: SocketAddr) -> io::Result<TcpStream> {
        init();
        new_socket(addr, SOCK_STREAM)
            .and_then(|socket| {
                // Required for a future `connect_overlapped` operation to be
                // executed successfully.
                let any_addr = inaddr_any(addr);
                let (raw_addr, raw_addr_length) = socket_addr(&any_addr);
                syscall!(
                    bind(socket, raw_addr, raw_addr_length),
                    PartialEq::eq,
                    SOCKET_ERROR
                )
                .and_then(|_| {
                    let (raw_addr, raw_addr_length) = socket_addr(&addr);
                    syscall!(
                        connect(socket, raw_addr, raw_addr_length),
                        PartialEq::eq,
                        SOCKET_ERROR
                    )
                    .or_else(|err| match err {
                        ref err if err.kind() == io::ErrorKind::WouldBlock => Ok(0),
                        err => Err(err),
                    })
                })
                .map(|_| socket)
                .map_err(|err| {
                    // Close the socket if we hit an error, ignoring the error
                    // from closing since we can't pass back two errors.
                    let _ = unsafe { closesocket(socket) };
                    err
                })
            })
            .map(|socket| TcpStream {
                state: SocketState::new(),
                inner: unsafe { net::TcpStream::from_raw_socket(socket as StdSocket) },
            })
    }

    pub fn from_std(inner: net::TcpStream) -> TcpStream {
        TcpStream {
            state: SocketState::new(),
            inner,
        }
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.inner.try_clone().map(|s| TcpStream {
            state: SocketState::new(),
            inner: s,
        })
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
        self.state.do_io(|| (&self.inner).read(buf))
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.state.do_io(|| (&self.inner).read_vectored(bufs))
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.state.do_io(|| (&self.inner).write(buf))
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.state.do_io(|| (&self.inner).write_vectored(bufs))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state.do_io(|| (&self.inner).flush())
    }
}

impl event::Source for TcpStream {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceSocket(&self.inner.as_raw_socket(), &mut self.state)
            .register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceSocket(&self.inner.as_raw_socket(), &mut self.state)
            .reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        SourceSocket(&self.inner.as_raw_socket(), &mut self.state).deregister(registry)
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl FromRawSocket for TcpStream {
    unsafe fn from_raw_socket(rawsocket: RawSocket) -> TcpStream {
        TcpStream {
            state: SocketState::new(),
            inner: net::TcpStream::from_raw_socket(rawsocket),
        }
    }
}

impl IntoRawSocket for TcpStream {
    fn into_raw_socket(self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

impl AsRawSocket for TcpStream {
    fn as_raw_socket(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

pub struct TcpListener {
    state: SocketState,
    inner: net::TcpListener,
}

impl TcpListener {
    pub fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
        init();
        new_socket(addr, SOCK_STREAM).and_then(|socket| {
            let (raw_addr, raw_addr_length) = socket_addr(&addr);
            syscall!(
                bind(socket, raw_addr, raw_addr_length,),
                PartialEq::eq,
                SOCKET_ERROR
            )
            .and_then(|_| syscall!(listen(socket, 1024), PartialEq::eq, SOCKET_ERROR))
            .map_err(|err| {
                // Close the socket if we hit an error, ignoring the error
                // from closing since we can't pass back two errors.
                let _ = unsafe { closesocket(socket) };
                err
            })
            .map(|_| TcpListener {
                state: SocketState::new(),
                inner: unsafe { net::TcpListener::from_raw_socket(socket as StdSocket) },
            })
        })
    }

    pub fn from_std(inner: net::TcpListener) -> TcpListener {
        TcpListener {
            state: SocketState::new(),
            inner,
        }
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.inner.try_clone().map(|s| TcpListener {
            state: SocketState::new(),
            inner: s,
        })
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        self.state
            .do_io(|| self.inner.accept())
            .and_then(|(inner, addr)| {
                inner.set_nonblocking(true).map(|()| {
                    (
                        TcpStream {
                            state: SocketState::new(),
                            inner,
                        },
                        addr,
                    )
                })
            })
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
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceSocket(&self.inner.as_raw_socket(), &mut self.state)
            .register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceSocket(&self.inner.as_raw_socket(), &mut self.state)
            .reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        SourceSocket(&self.inner.as_raw_socket(), &mut self.state).deregister(registry)
    }
}

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl FromRawSocket for TcpListener {
    unsafe fn from_raw_socket(rawsocket: RawSocket) -> TcpListener {
        TcpListener {
            state: SocketState::new(),
            inner: net::TcpListener::from_raw_socket(rawsocket),
        }
    }
}

impl IntoRawSocket for TcpListener {
    fn into_raw_socket(self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

impl AsRawSocket for TcpListener {
    fn as_raw_socket(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}
