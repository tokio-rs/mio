use io::MapNonBlock;
use std::cell::Cell;
use std::io::{Read, Write};
use std::net::{self, SocketAddr};
use std::os::unix::io::{RawFd, FromRawFd, AsRawFd};

use libc;
use net2::{TcpStreamExt, TcpListenerExt};
use nix::fcntl::FcntlArg::F_SETFL;
use nix::fcntl::{fcntl, O_NONBLOCK};

use {io, Evented, EventSet, PollOpt, Selector, Token, TryAccept};
use sys::unix::eventedfd::EventedFd;

#[derive(Debug)]
pub struct TcpStream {
    inner: net::TcpStream,
    selector_id: Cell<Option<usize>>,
}

#[derive(Debug)]
pub struct TcpListener {
    inner: net::TcpListener,
    selector_id: Cell<Option<usize>>,
}

fn set_nonblock(s: &AsRawFd) -> io::Result<()> {
    fcntl(s.as_raw_fd(), F_SETFL(O_NONBLOCK)).map_err(super::from_nix_error)
                                             .map(|_| ())
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
            selector_id: Cell::new(None),
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
                selector_id: self.selector_id.clone(),
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

    fn associate_selector(&self, selector: &Selector) -> io::Result<()> {
        let selector_id = self.selector_id.get();

        if selector_id.is_some() && selector_id != Some(selector.id()) {
            Err(io::Error::new(io::ErrorKind::Other, "socket already registered"))
        } else {
            self.selector_id.set(Some(selector.id()));
            Ok(())
        }
    }
}

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Evented for TcpStream {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        try!(self.associate_selector(selector));
        EventedFd(&self.as_raw_fd()).register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(selector)
    }
}

impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpStream {
        TcpStream {
            inner: net::TcpStream::from_raw_fd(fd),
            selector_id: Cell::new(None),
        }
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
            selector_id: Cell::new(None),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.inner.try_clone().map(|s| {
            TcpListener {
                inner: s,
                selector_id: self.selector_id.clone(),
            }
        })
    }

    pub fn accept(&self) -> io::Result<Option<(TcpStream, SocketAddr)>> {
        self.inner.accept().and_then(|(s, a)| {
            try!(set_nonblock(&s));
            Ok((TcpStream {
                inner: s,
                selector_id: Cell::new(None),
            }, a))
        }).map_non_block()
    }

    pub fn take_socket_error(&self) -> io::Result<()> {
        self.inner.take_error().and_then(|e| {
            match e {
                Some(e) => Err(e),
                None => Ok(())
            }
        })
    }

    fn associate_selector(&self, selector: &Selector) -> io::Result<()> {
        let selector_id = self.selector_id.get();

        if selector_id.is_some() && selector_id != Some(selector.id()) {
            Err(io::Error::new(io::ErrorKind::Other, "socket already registered"))
        } else {
            self.selector_id.set(Some(selector.id()));
            Ok(())
        }
    }
}

impl Evented for TcpListener {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        try!(self.associate_selector(selector));
        EventedFd(&self.as_raw_fd()).register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(selector)
    }
}

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpListener {
        TcpListener {
            inner: net::TcpListener::from_raw_fd(fd),
            selector_id: Cell::new(None),
        }
    }
}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
