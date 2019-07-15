use crate::poll;
use crate::sys::Selector;
use crate::{event, Interests, Registry, Token};

use iovec::IoVec;
use net2::TcpStreamExt;

use std::fmt;
use std::io::{self, Read, Write};
use std::net::{self, SocketAddr};
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::sync::{Arc, RwLock};
use std::time::Duration;

struct RegistryInternalStruct {
    selector: Option<Arc<Selector>>,
    token: Option<Token>,
    interests: Option<Interests>,
}

pub struct TcpStream {
    internal: Arc<RwLock<RegistryInternalStruct>>,
    inner: net::TcpStream,
}

pub struct TcpListener {
    internal: Arc<RwLock<RegistryInternalStruct>>,
    inner: net::TcpListener,
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
            internal: Arc::new(RwLock::new(RegistryInternalStruct {
                selector: None,
                token: None,
                interests: None,
            })),
            inner: stream,
        })
    }

    pub fn from_stream(stream: net::TcpStream) -> TcpStream {
        TcpStream {
            internal: Arc::new(RwLock::new(RegistryInternalStruct {
                selector: None,
                token: None,
                interests: None,
            })),
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
            internal: Arc::new(RwLock::new(RegistryInternalStruct {
                selector: None,
                token: None,
                interests: None,
            })),
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

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let result = self.inner.read(buf);
        if let Err(ref e) = result {
            if e.kind() == io::ErrorKind::WouldBlock {
                let internal = self.internal.read().unwrap();
                if let Some(selector) = &internal.selector {
                    selector.reregister(
                        self,
                        internal.token.unwrap(),
                        internal.interests.unwrap(),
                    )?;
                }
            }
        }
        result
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let result = self.inner.write(buf);
        if let Err(ref e) = result {
            if e.kind() == io::ErrorKind::WouldBlock {
                let internal = self.internal.read().unwrap();
                if let Some(selector) = &internal.selector {
                    selector.reregister(
                        self,
                        internal.token.unwrap(),
                        internal.interests.unwrap(),
                    )?;
                }
            }
        }
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        let result = self.inner.flush();
        if let Err(ref e) = result {
            if e.kind() == io::ErrorKind::WouldBlock {
                let internal = self.internal.read().unwrap();
                if let Some(selector) = &internal.selector {
                    selector.reregister(
                        self,
                        internal.token.unwrap(),
                        internal.interests.unwrap(),
                    )?;
                }
            }
        }
        result
    }
}

impl event::Source for TcpStream {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        let result = poll::selector(registry).register(self, token, interests);
        match result {
            Ok(_) => {
                let mut internal = self.internal.write().unwrap();
                internal.selector = Some(poll::selector(registry));
                internal.token = Some(token);
                internal.interests = Some(interests);
            }
            Err(_) => {}
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
                internal.selector = Some(poll::selector(registry));
                internal.token = Some(token);
                internal.interests = Some(interests);
            }
            Err(_) => {}
        }
        result
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        {
            let mut internal = self.internal.write().unwrap();
            internal.selector = None;
            internal.token = None;
            internal.interests = None;
        }
        poll::selector(registry).deregister(self)
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
            internal: Arc::new(RwLock::new(RegistryInternalStruct {
                selector: None,
                token: None,
                interests: None,
            })),
            inner: net::TcpStream::from_raw_socket(rawsocket),
        }
    }
}

impl IntoRawSocket for TcpStream {
    fn into_raw_socket(self) -> RawSocket {
        self.inner.into_raw_socket()
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
            internal: Arc::new(RwLock::new(RegistryInternalStruct {
                selector: None,
                token: None,
                interests: None,
            })),
            inner,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.inner.try_clone().map(|s| TcpListener {
            internal: Arc::new(RwLock::new(RegistryInternalStruct {
                selector: None,
                token: None,
                interests: None,
            })),
            inner: s,
        })
    }

    pub fn accept(&self) -> io::Result<(net::TcpStream, SocketAddr)> {
        let result = self.inner.accept();
        if let Err(ref e) = result {
            if e.kind() == io::ErrorKind::WouldBlock {
                let internal = self.internal.read().unwrap();
                if let Some(selector) = &internal.selector {
                    selector.reregister(
                        self,
                        internal.token.unwrap(),
                        internal.interests.unwrap(),
                    )?;
                }
            }
        }
        result
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
        let result = poll::selector(registry).register(self, token, interests);
        match result {
            Ok(_) => {
                let mut internal = self.internal.write().unwrap();
                internal.selector = Some(poll::selector(registry));
                internal.token = Some(token);
                internal.interests = Some(interests);
            }
            Err(_) => {}
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
                internal.selector = Some(poll::selector(registry));
                internal.token = Some(token);
                internal.interests = Some(interests);
            }
            Err(_) => {}
        }
        result
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        {
            let mut internal = self.internal.write().unwrap();
            internal.selector = None;
            internal.token = None;
            internal.interests = None;
        }
        poll::selector(registry).deregister(self)
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
            internal: Arc::new(RwLock::new(RegistryInternalStruct {
                selector: None,
                token: None,
                interests: None,
            })),
            inner: net::TcpListener::from_raw_socket(rawsocket),
        }
    }
}

impl IntoRawSocket for TcpListener {
    fn into_raw_socket(self) -> RawSocket {
        self.inner.into_raw_socket()
    }
}

impl AsRawSocket for TcpListener {
    fn as_raw_socket(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}
