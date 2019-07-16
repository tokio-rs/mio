use crate::poll;
use crate::{event, Interests, Registry, Token};

use iovec::IoVec;
use net2::TcpStreamExt;

use std::fmt;
use std::io::{self, Read, Write};
use std::net::{self, SocketAddr};
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use super::selector::{Selector, SockState};

struct InternalState {
    selector: Arc<Selector>,
    token: Token,
    interests: Interests,
    sock_state: Option<Arc<Mutex<SockState>>>,
}

impl InternalState {
    fn new(selector: Arc<Selector>, token: Token, interests: Interests) -> InternalState {
        InternalState {
            selector,
            token,
            interests,
            sock_state: None,
        }
    }
}

pub struct TcpStream {
    internal: Arc<RwLock<Option<InternalState>>>,
    inner: net::TcpStream,
}

pub struct TcpListener {
    internal: Arc<RwLock<Option<InternalState>>>,
    inner: net::TcpListener,
}

macro_rules! wouldblock {
    ($self:ident, $method:ident)  => {{
        let result = (&$self.inner).$method();
        if let Err(ref e) = result {
            if e.kind() == io::ErrorKind::WouldBlock {
                let internal = $self.internal.read().unwrap();
                if let Some(internal) = &*internal {
                    internal.selector.reregister(
                        $self,
                        internal.token,
                        internal.interests,
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
                let internal = $self.internal.read().unwrap();
                if let Some(internal) = &*internal {
                    internal.selector.reregister(
                        $self,
                        internal.token,
                        internal.interests,
                    )?;
                }
            }
        }
        result
    }};
}

impl TcpStream {
    pub fn connect(stream: net::TcpStream, addr: SocketAddr) -> io::Result<TcpStream> {
        stream.set_nonblocking(true)?;

        match stream.connect(addr) {
            Ok(..) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }

        Ok(TcpStream {
            internal: Arc::new(RwLock::new(None)),
            inner: stream,
        })
    }

    pub fn from_stream(stream: net::TcpStream) -> TcpStream {
        TcpStream {
            internal: Arc::new(RwLock::new(None)),
            inner: stream,
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
            internal: Arc::new(RwLock::new(None)),
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

    pub fn readv(&mut self, bufs: &mut [&mut IoVec]) -> io::Result<usize> {
        let mut amt = 0;
        for buf in bufs {
            match self.read(buf) {
                // If we did a partial read, then return what we've read so far
                Ok(n) if n < buf.len() => return Ok(amt + n),

                // Otherwise filled this buffer entirely, so try to fill the
                // next one as well.
                Ok(n) => amt += n,

                Err(e) => {
                    if amt > 0 {
                        return Ok(amt);
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Ok(amt)
    }

    pub fn writev(&mut self, bufs: &[&IoVec]) -> io::Result<usize> {
        let len = bufs.iter().map(|b| b.len()).fold(0, |a, b| a + b);
        let mut writebuf = Vec::with_capacity(len);
        for buf in bufs {
            writebuf.extend_from_slice(buf);
        }
        self.write(&writebuf)
    }
}

impl super::MioSocketState for TcpStream {
    fn get_sock_state(&self) -> Option<Arc<Mutex<SockState>>> {
        let internal = self.internal.read().unwrap();
        match &*internal {
            Some(internal) => match &internal.sock_state {
                Some(arc) => Some(arc.clone()),
                None => None,
            },
            None => None,
        }
    }
    fn set_sock_state(&self, sock_state: Option<Arc<Mutex<SockState>>>) {
        let mut internal = self.internal.write().unwrap();
        match &mut *internal {
            Some(internal) => {
                internal.sock_state = sock_state;
            }
            None => {}
        };
    }
}

impl<'a> super::MioSocketState for &'a TcpStream {
    fn get_sock_state(&self) -> Option<Arc<Mutex<SockState>>> {
        let internal = self.internal.read().unwrap();
        match &*internal {
            Some(internal) => match &internal.sock_state {
                Some(arc) => Some(arc.clone()),
                None => None,
            },
            None => None,
        }
    }
    fn set_sock_state(&self, sock_state: Option<Arc<Mutex<SockState>>>) {
        let mut internal = self.internal.write().unwrap();
        match &mut *internal {
            Some(internal) => {
                internal.sock_state = sock_state;
            }
            None => {}
        };
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        let internal = self.internal.read().unwrap();
        if let Some(internal) = internal.as_ref() {
            if let Some(sock_state) = internal.sock_state.as_ref() {
                internal
                    .selector
                    .inner()
                    .mark_delete_socket(sock_state.clone());
            }
        }
    }
}

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        wouldblock!(self, read, buf)
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        wouldblock!(self, read, buf)
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        wouldblock!(self, write, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        wouldblock!(self, flush)
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        wouldblock!(self, write, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        wouldblock!(self, flush)
    }
}

impl event::Source for TcpStream {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        {
            let mut internal = self.internal.write().unwrap();
            if internal.is_none() {
                *internal = Some(InternalState::new(
                    poll::selector_arc(registry),
                    token,
                    interests,
                ));
            }
        }
        let result = poll::selector(registry).register(self, token, interests);
        match result {
            Ok(_) => {}
            Err(_) => {
                let mut internal = self.internal.write().unwrap();
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
                let mut internal = self.internal.write().unwrap();
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
                let mut internal = self.internal.write().unwrap();
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
            internal: Arc::new(RwLock::new(None)),
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
    pub fn new(inner: net::TcpListener) -> io::Result<TcpListener> {
        inner.set_nonblocking(true)?;
        Ok(TcpListener {
            internal: Arc::new(RwLock::new(None)),
            inner,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.inner.try_clone().map(|s| TcpListener {
            internal: Arc::new(RwLock::new(None)),
            inner: s,
        })
    }

    pub fn accept(&self) -> io::Result<(net::TcpStream, SocketAddr)> {
        wouldblock!(self, accept)
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

impl Drop for TcpListener {
    fn drop(&mut self) {
        let internal = self.internal.read().unwrap();
        if let Some(internal) = internal.as_ref() {
            if let Some(sock_state) = internal.sock_state.as_ref() {
                internal
                    .selector
                    .inner()
                    .mark_delete_socket(sock_state.clone());
            }
        }
    }
}

impl super::MioSocketState for TcpListener {
    fn get_sock_state(&self) -> Option<Arc<Mutex<SockState>>> {
        let internal = self.internal.read().unwrap();
        match &*internal {
            Some(internal) => match &internal.sock_state {
                Some(arc) => Some(arc.clone()),
                None => None,
            },
            None => None,
        }
    }
    fn set_sock_state(&self, sock_state: Option<Arc<Mutex<SockState>>>) {
        let mut internal = self.internal.write().unwrap();
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
            let mut internal = self.internal.write().unwrap();
            if internal.is_none() {
                *internal = Some(InternalState::new(
                    poll::selector_arc(registry),
                    token,
                    interests,
                ));
            }
        }
        let result = poll::selector(registry).register(self, token, interests);
        match result {
            Ok(_) => {}
            Err(_) => {
                let mut internal = self.internal.write().unwrap();
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
                let mut internal = self.internal.write().unwrap();
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
                let mut internal = self.internal.write().unwrap();
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
            internal: Arc::new(RwLock::new(None)),
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
