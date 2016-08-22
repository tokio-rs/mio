use std::io::{Read, Write};
use std::net::{self, SocketAddr};
use std::os::unix::io::{RawFd, FromRawFd, IntoRawFd, AsRawFd};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;

use libc;
use net2::TcpStreamExt;

#[allow(unused_imports)]
use net2::TcpListenerExt;

use {io, poll, Evented, EventSet, Poll, PollOpt, Token};

use sys::unix::eventedfd::EventedFd;
use sys::unix::io::set_nonblock;

#[derive(Debug)]
pub struct TcpStream {
    inner: net::TcpStream,
    selector_id: AtomicUsize,
}

#[derive(Debug)]
pub struct TcpListener {
    inner: net::TcpListener,
    selector_id: AtomicUsize,
}

impl TcpStream {
    pub fn connect(stream: net::TcpStream, addr: &SocketAddr) -> io::Result<TcpStream> {
        try!(set_nonblock(&stream));

        match stream.connect(addr) {
            Ok(..) => {}
            Err(ref e) if e.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(e) => return Err(e),
        }

        Ok(TcpStream {
            inner: stream,
            selector_id: AtomicUsize::new(0),
        })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.inner.try_clone().map(|s| {
            TcpStream {
                inner: s,
                selector_id: AtomicUsize::new(self.selector_id.load(SeqCst)),
            }
        })
    }

    pub fn shutdown(&self, how: net::Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        TcpStreamExt::set_nodelay(&self.inner, nodelay)
    }

    pub fn set_keepalive(&self, seconds: Option<u32>) -> io::Result<()> {
        self.inner.set_keepalive_ms(seconds.map(|s| s * 1000))
    }

    pub fn take_socket_error(&self) -> io::Result<()> {
        self.inner.take_error().and_then(|e| {
            match e {
                Some(e) => Err(e),
                None => Ok(())
            }
        })
    }

    fn associate_selector(&self, poll: &Poll) -> io::Result<()> {
        let selector_id = self.selector_id.load(SeqCst);

        if selector_id != 0 && selector_id != poll::selector(poll).id() {
            Err(io::Error::new(io::ErrorKind::Other, "socket already registered"))
        } else {
            self.selector_id.store(poll::selector(poll).id(), SeqCst);
            Ok(())
        }
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.inner).read(buf)
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.inner).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.inner).flush()
    }
}

impl Evented for TcpStream {
    fn register(&self, poll: &Poll, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        try!(self.associate_selector(poll));
        EventedFd(&self.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}

impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpStream {
        TcpStream {
            inner: net::TcpStream::from_raw_fd(fd),
            selector_id: AtomicUsize::new(0),
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
    pub fn new(inner: net::TcpListener, _addr: &SocketAddr) -> io::Result<TcpListener> {
        try!(set_nonblock(&inner));
        Ok(TcpListener {
            inner: inner,
            selector_id: AtomicUsize::new(0),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.inner.try_clone().map(|s| {
            TcpListener {
                inner: s,
                selector_id: AtomicUsize::new(self.selector_id.load(SeqCst)),
            }
        })
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        self.inner.accept().and_then(|(s, a)| {
            try!(set_nonblock(&s));
            Ok((TcpStream {
                inner: s,
                selector_id: AtomicUsize::new(0),
            }, a))
        })
    }

    pub fn take_socket_error(&self) -> io::Result<()> {
        self.inner.take_error().and_then(|e| {
            match e {
                Some(e) => Err(e),
                None => Ok(())
            }
        })
    }

    fn associate_selector(&self, poll: &Poll) -> io::Result<()> {
        let selector_id = self.selector_id.load(SeqCst);

        if selector_id != 0 && selector_id != poll::selector(poll).id() {
            Err(io::Error::new(io::ErrorKind::Other, "socket already registered"))
        } else {
            self.selector_id.store(poll::selector(poll).id(), SeqCst);
            Ok(())
        }
    }
}

impl Evented for TcpListener {
    fn register(&self, poll: &Poll, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        try!(self.associate_selector(poll));
        EventedFd(&self.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpListener {
        TcpListener {
            inner: net::TcpListener::from_raw_fd(fd),
            selector_id: AtomicUsize::new(0),
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

