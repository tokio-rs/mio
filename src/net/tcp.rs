use std::io::{Read, Write};
use std::net::{self, SocketAddr, SocketAddrV4, SocketAddrV6, Ipv4Addr, Ipv6Addr};

use net2::TcpBuilder;

use {io, sys, Evented, EventSet, PollOpt, Selector, Token, TryAccept};

/*
 *
 * ===== TcpStream =====
 *
 */

#[derive(Debug)]
pub struct TcpStream {
    sys: sys::TcpStream,
}

pub use std::net::Shutdown;

impl TcpStream {
    /// Create a new TCP stream an issue a non-blocking connect to the specified
    /// address.
    ///
    /// This convenience method is available and uses the system's default
    /// options when creating a socket which is then conntected. If fine-grained
    /// control over the creation of the socket is desired, you can use
    /// `net2::TcpBuilder` to configure a socket and then pass its socket to
    /// `TcpStream::connect_stream` to transfer ownership into mio and schedule
    /// the connect operation.
    pub fn connect(addr: &SocketAddr) -> io::Result<TcpStream> {
        let sock = try!(match *addr {
            SocketAddr::V4(..) => TcpBuilder::new_v4(),
            SocketAddr::V6(..) => TcpBuilder::new_v6(),
        });
        // Required on Windows for a future `connect_overlapped` operation to be
        // executed successfully.
        if cfg!(windows) {
            try!(sock.bind(&inaddr_any(addr)));
        }
        TcpStream::connect_stream(try!(sock.to_tcp_stream()), addr)
    }

    /// Creates a new `TcpStream` from the pending socket inside the given
    /// `std::net::TcpBuilder`, connecting it to the address specified.
    ///
    /// This constructor allows configuring the socket before it's actually
    /// connected, and this function will transfer ownership to the returned
    /// `TcpStream` if successful. An unconnected `TcpStream` can be created
    /// with the `net2::TcpBuilder` type (and also configured via that route).
    ///
    /// The platform specific behavior of this function looks like:
    ///
    /// * On Unix, the socket is placed into nonblocking mode and then a
    ///   `connect` call is issued.
    ///
    /// * On Windows, the address is stored internally and the connect operation
    ///   is issued when the returned `TcpStream` is registered with an event
    ///   loop. Note that on Windows you must `bind` a socket before it can be
    ///   connected, so if a custom `TcpBuilder` is used it should be bound
    ///   (perhaps to `INADDR_ANY`) before this method is called.
    pub fn connect_stream(stream: net::TcpStream,
                          addr: &SocketAddr) -> io::Result<TcpStream> {
        Ok(TcpStream {
            sys: try!(sys::TcpStream::connect(stream, addr)),
        })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.sys.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sys.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.sys.try_clone().map(|s| TcpStream { sys: s })
    }
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.sys.shutdown(how)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.sys.set_nodelay(nodelay)
    }

    pub fn set_keepalive(&self, seconds: Option<u32>) -> io::Result<()> {
        self.sys.set_keepalive(seconds)
    }

    pub fn take_socket_error(&self) -> io::Result<()> {
        self.sys.take_socket_error()
    }
}

fn inaddr_any(other: &SocketAddr) -> SocketAddr {
    match *other {
        SocketAddr::V4(..) => {
            let any = Ipv4Addr::new(0, 0, 0, 0);
            let addr = SocketAddrV4::new(any, 0);
            SocketAddr::V4(addr)
        }
        SocketAddr::V6(..) => {
            let any = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0);
            let addr = SocketAddrV6::new(any, 0, 0, 0);
            SocketAddr::V6(addr)
        }
    }
}

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.sys.read(buf)
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.sys.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sys.flush()
    }
}

impl Evented for TcpStream {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sys.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sys.deregister(selector)
    }
}

/*
 *
 * ===== TcpListener =====
 *
 */

#[derive(Debug)]
pub struct TcpListener {
    sys: sys::TcpListener,
}

impl TcpListener {
    /// Convenience method to bind a new TCP listener to the specified address
    /// to receive new connections.
    ///
    /// This function will take the following steps:
    ///
    /// 1. Create a new TCP socket.
    /// 2. Set the `SO_REUSEADDR` option on the socket.
    /// 3. Bind the socket to the specified address.
    /// 4. Call `listen` on the socket to prepare it to receive new connections.
    ///
    /// If fine-grained control over the binding and listening process for a
    /// socket is desired then the `net2::TcpBuilder` methods can be used in
    /// combination with the `TcpListener::from_listener` method to transfer
    /// ownership into mio.
    pub fn bind(addr: &SocketAddr) -> io::Result<TcpListener> {
        // Create the socket
        let sock = try!(match *addr {
            SocketAddr::V4(..) => TcpBuilder::new_v4(),
            SocketAddr::V6(..) => TcpBuilder::new_v6(),
        });

        // Set SO_REUSEADDR
        try!(sock.reuse_address(true));

        // Bind the socket
        try!(sock.bind(addr));

        // listen
        let listener = try!(sock.listen(1024));
        Ok(TcpListener {
            sys: try!(sys::TcpListener::new(listener, addr)),
        })
    }

    /// Creates a new `TcpListener` from an instance of a
    /// `std::net::TcpListener` type.
    ///
    /// This function will set the `listener` provided into nonblocking mode on
    /// Unix, and otherwise the stream will just be wrapped up in an mio stream
    /// ready to accept new connections and become associated with an event
    /// loop.
    ///
    /// The address provided must be the address that the listener is bound to.
    pub fn from_listener(listener: net::TcpListener, addr: &SocketAddr)
                         -> io::Result<TcpListener> {
        sys::TcpListener::new(listener, addr).map(|s| TcpListener { sys: s })
    }

    /// Accepts a new `TcpStream`.
    ///
    /// Returns a `Ok(None)` when the socket `WOULDBLOCK`, this means the stream
    /// will be ready at a later point. If an accepted stream is returned, the
    /// address of the peer is returned along with it
    pub fn accept(&self) -> io::Result<Option<(TcpStream, SocketAddr)>> {
        self.sys.accept().map(|o| o.map(|(s, a)| (TcpStream { sys: s }, a)))
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sys.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.sys.try_clone().map(|s| TcpListener { sys: s })
    }

    pub fn take_socket_error(&self) -> io::Result<()> {
        self.sys.take_socket_error()
    }
}

impl Evented for TcpListener {
    fn register(&self, selector: &mut Selector, token: Token,
                interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sys.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token,
                  interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.sys.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.sys.deregister(selector)
    }
}

impl TryAccept for TcpListener {
    type Output = TcpStream;

    fn accept(&self) -> io::Result<Option<TcpStream>> {
        TcpListener::accept(self).map(|a| a.map(|(s, _)| s))
    }
}

/*
 *
 * ===== UNIX ext =====
 *
 */

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

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
