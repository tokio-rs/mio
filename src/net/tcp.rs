use {io, sys, Evented, Interest, PollOpt, Selector, Token, TryRead, TryWrite};
use std::net::SocketAddr;

/*
 *
 * ===== TcpSocket =====
 *
 */

#[derive(Debug)]
pub struct TcpSocket {
    sys: sys::TcpSocket,
}

impl TcpSocket {
    /// Returns a new, unbound, non-blocking, IPv4 socket
    pub fn v4() -> io::Result<TcpSocket> {
        sys::TcpSocket::v4().map(From::from)
    }

    /// Returns a new, unbound, non-blocking, IPv6 socket
    pub fn v6() -> io::Result<TcpSocket> {
        sys::TcpSocket::v6().map(From::from)
    }

    pub fn connect(self, addr: &SocketAddr) -> io::Result<(TcpStream, bool)> {
        let complete = try!(self.sys.connect(addr));
        Ok((From::from(self.sys), complete))
    }

    pub fn bind(&self, addr: &SocketAddr) -> io::Result<()> {
        self.sys.bind(addr)
    }

    pub fn listen(self, backlog: usize) -> io::Result<TcpListener> {
        try!(self.sys.listen(backlog));
        Ok(From::from(self.sys))
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.sys.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sys.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpSocket> {
        self.sys.try_clone()
            .map(From::from)
    }

    /*
     *
     * ===== Socket Options =====
     *
     */

    pub fn set_reuseaddr(&self, val: bool) -> io::Result<()> {
        self.sys.set_reuseaddr(val)
    }
}

impl Evented for TcpSocket {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sys.deregister(selector)
    }
}

impl From<sys::TcpSocket> for TcpSocket {
    fn from(sys: sys::TcpSocket) -> TcpSocket {
        TcpSocket { sys: sys }
    }
}

/*
 *
 * ===== TcpStream =====
 *
 */

pub struct TcpStream {
    sys: sys::TcpSocket,
}

impl TcpStream {
    pub fn connect(addr: &SocketAddr) -> io::Result<TcpStream> {
        let sock = try!(match *addr {
            SocketAddr::V4(..) => TcpSocket::v4(),
            SocketAddr::V6(..) => TcpSocket::v6(),
        });

        sock.connect(addr)
            .map(|(stream, _)| stream)
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.sys.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sys.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.sys.try_clone()
            .map(From::from)
    }
}

impl TryRead for TcpStream {
    fn read_slice(&mut self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        self.sys.read_slice(buf)
    }
}

impl TryWrite for TcpStream {
    fn write_slice(&mut self, buf: &[u8]) -> io::Result<Option<usize>> {
        self.sys.write_slice(buf)
    }
}

impl Evented for TcpStream {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sys.deregister(selector)
    }
}

impl From<sys::TcpSocket> for TcpStream {
    fn from(sys: sys::TcpSocket) -> TcpStream {
        TcpStream { sys: sys }
    }
}

/*
 *
 * ===== TcpListener =====
 *
 */

pub struct TcpListener {
    sys: sys::TcpSocket,
}

impl TcpListener {
    pub fn bind(addr: &SocketAddr) -> io::Result<TcpListener> {
        // Create the socket
        let sock = try!(match *addr {
            SocketAddr::V4(..) => TcpSocket::v4(),
            SocketAddr::V6(..) => TcpSocket::v6(),
        });

        // Bind the socket
        try!(sock.bind(addr));

        // listen
        sock.listen(1024)
    }

    /// Accepts a new `TcpStream`.
    ///
    /// Returns a `Ok(None)` when the socket `WOULDBLOCK`, this means the stream will be ready at
    /// a later point.
    pub fn accept(&self) -> io::Result<Option<TcpStream>> {
        self.sys.accept()
            .map(|opt| {
                opt.map(|sys| TcpStream { sys: sys })
            })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sys.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.sys.try_clone()
            .map(From::from)
    }
}

impl From<sys::TcpSocket> for TcpListener {
    fn from(sys: sys::TcpSocket) -> TcpListener {
        TcpListener { sys: sys }
    }
}

impl Evented for TcpListener {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sys.deregister(selector)
    }
}

/*
 *
 * ===== UNIX ext =====
 *
 */

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};

#[cfg(unix)]
use unix::FromRawFd;

#[cfg(unix)]
impl AsRawFd for TcpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

#[cfg(unix)]
impl FromRawFd for TcpSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpSocket {
        TcpSocket { sys: FromRawFd::from_raw_fd(fd) }
    }
}

#[cfg(unix)]
impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

#[cfg(unix)]
impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpStream {
        TcpStream { sys: FromRawFd::from_raw_fd(fd) }
    }
}

#[cfg(unix)]
impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.sys.as_raw_fd()
    }
}

#[cfg(unix)]
impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpListener {
        TcpListener { sys: FromRawFd::from_raw_fd(fd) }
    }
}
