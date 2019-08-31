use super::selector::SockState;
use super::InternalState;
use super::{inaddr_any, new_socket, socket_addr};
use crate::poll;
use crate::sys::windows::init;
use crate::{event, Interests, Registry, Token};

use std::fmt;
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::{self, SocketAddr};
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::os::windows::raw::SOCKET as StdSocket; // winapi uses usize, stdlib uses u32/u64.
use std::sync::{Arc, Mutex};
use winapi::um::winsock2::{bind, closesocket, connect, listen, SOCKET_ERROR, SOCK_STREAM};

pub struct TcpStream {
    internal: Arc<Mutex<Option<InternalState>>>,
    inner: net::TcpStream,
}

pub struct TcpListener {
    internal: Arc<Mutex<Option<InternalState>>>,
    inner: net::TcpListener,
}

macro_rules! wouldblock {
    ($self:ident, $method:ident)  => {{
        let result = (&$self.inner).$method();
        if let Err(ref e) = result {
            if e.kind() == io::ErrorKind::WouldBlock {
                let internal = $self.internal.lock().unwrap();
                if internal.is_some() {
                    let selector = internal.as_ref().unwrap().selector.clone();
                    let token = internal.as_ref().unwrap().token;
                    let interests = internal.as_ref().unwrap().interests;
                    drop(internal);
                    selector.reregister(
                        $self,
                        token,
                        interests,
                    )?;
                }
            }
        }
        result
    }};
    ($self:ident, $method:ident, $($args:expr),* )  => {{
        let result = (&$self.inner).$method($($args),*);
        if let Err(ref e) = result {
            if e.kind() == io::ErrorKind::WouldBlock {
                let internal = $self.internal.lock().unwrap();
                if internal.is_some() {
                    let selector = internal.as_ref().unwrap().selector.clone();
                    let token = internal.as_ref().unwrap().token;
                    let interests = internal.as_ref().unwrap().interests;
                    drop(internal);
                    selector.reregister(
                        $self,
                        token,
                        interests,
                    )?;
                }
            }
        }
        result
    }};
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
                internal: Arc::new(Mutex::new(None)),
                inner: unsafe { net::TcpStream::from_raw_socket(socket as StdSocket) },
            })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.inner.try_clone().map(|s| TcpStream {
            internal: Arc::new(Mutex::new(None)),
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

impl super::SocketState for TcpStream {
    fn get_sock_state(&self) -> Option<Arc<Mutex<SockState>>> {
        let internal = self.internal.lock().unwrap();
        match &*internal {
            Some(internal) => match &internal.sock_state {
                Some(arc) => Some(arc.clone()),
                None => None,
            },
            None => None,
        }
    }
    fn set_sock_state(&self, sock_state: Option<Arc<Mutex<SockState>>>) {
        let mut internal = self.internal.lock().unwrap();
        match &mut *internal {
            Some(internal) => {
                internal.sock_state = sock_state;
            }
            None => {}
        };
    }
}

impl<'a> super::SocketState for &'a TcpStream {
    fn get_sock_state(&self) -> Option<Arc<Mutex<SockState>>> {
        let internal = self.internal.lock().unwrap();
        match &*internal {
            Some(internal) => match &internal.sock_state {
                Some(arc) => Some(arc.clone()),
                None => None,
            },
            None => None,
        }
    }
    fn set_sock_state(&self, sock_state: Option<Arc<Mutex<SockState>>>) {
        let mut internal = self.internal.lock().unwrap();
        match &mut *internal {
            Some(internal) => {
                internal.sock_state = sock_state;
            }
            None => {}
        };
    }
}

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        wouldblock!(self, read, buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        wouldblock!(self, read_vectored, bufs)
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        wouldblock!(self, read, buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        wouldblock!(self, read_vectored, bufs)
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        wouldblock!(self, write, buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        wouldblock!(self, write_vectored, bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        wouldblock!(self, flush)
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        wouldblock!(self, write, buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        wouldblock!(self, write_vectored, bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        wouldblock!(self, flush)
    }
}

impl event::Source for TcpStream {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        {
            let mut internal = self.internal.lock().unwrap();
            if internal.is_none() {
                *internal = Some(InternalState::new(
                    poll::selector(registry).clone_inner(),
                    token,
                    interests,
                ));
            }
        }
        let result = poll::selector(registry).register(self, token, interests);
        match result {
            Ok(_) => {}
            Err(_) => {
                let mut internal = self.internal.lock().unwrap();
                *internal = None;
            }
        }
        result
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        let result = poll::selector(registry).reregister(self, token, interests);
        match result {
            Ok(_) => {
                let mut internal = self.internal.lock().unwrap();
                internal.as_mut().unwrap().token = token;
                internal.as_mut().unwrap().interests = interests;
            }
            Err(_) => {}
        };
        result
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        let result = poll::selector(registry).deregister(self);
        match result {
            Ok(_) => {
                let mut internal = self.internal.lock().unwrap();
                *internal = None;
            }
            Err(_) => {}
        };
        result
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
            internal: Arc::new(Mutex::new(None)),
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
                internal: Arc::new(Mutex::new(None)),
                inner: unsafe { net::TcpListener::from_raw_socket(socket as StdSocket) },
            })
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.inner.try_clone().map(|s| TcpListener {
            internal: Arc::new(Mutex::new(None)),
            inner: s,
        })
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        wouldblock!(self, accept).and_then(|(inner, addr)| {
            inner.set_nonblocking(true).map(|()| {
                (
                    TcpStream {
                        internal: Arc::new(Mutex::new(None)),
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

impl super::SocketState for TcpListener {
    fn get_sock_state(&self) -> Option<Arc<Mutex<SockState>>> {
        let internal = self.internal.lock().unwrap();
        match &*internal {
            Some(internal) => match &internal.sock_state {
                Some(arc) => Some(arc.clone()),
                None => None,
            },
            None => None,
        }
    }
    fn set_sock_state(&self, sock_state: Option<Arc<Mutex<SockState>>>) {
        let mut internal = self.internal.lock().unwrap();
        match &mut *internal {
            Some(internal) => {
                internal.sock_state = sock_state;
            }
            None => {}
        };
    }
}

impl event::Source for TcpListener {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        {
            let mut internal = self.internal.lock().unwrap();
            if internal.is_none() {
                *internal = Some(InternalState::new(
                    poll::selector(registry).clone_inner(),
                    token,
                    interests,
                ));
            }
        }
        let result = poll::selector(registry).register(self, token, interests);
        match result {
            Ok(_) => {}
            Err(_) => {
                let mut internal = self.internal.lock().unwrap();
                *internal = None;
            }
        }
        result
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        let result = poll::selector(registry).reregister(self, token, interests);
        match result {
            Ok(_) => {
                let mut internal = self.internal.lock().unwrap();
                internal.as_mut().unwrap().token = token;
                internal.as_mut().unwrap().interests = interests;
            }
            Err(_) => {}
        };
        result
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        let result = poll::selector(registry).deregister(self);
        match result {
            Ok(_) => {
                let mut internal = self.internal.lock().unwrap();
                *internal = None;
            }
            Err(_) => {}
        };
        result
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
            internal: Arc::new(Mutex::new(None)),
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
