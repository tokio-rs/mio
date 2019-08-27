use std::fmt;
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

#[cfg(debug_assertions)]
use crate::poll::SelectorId;
use crate::{event, sys, Interests, Registry, Token};

/// A non-blocking TCP stream between a local socket and a remote socket.
///
/// The socket will be closed when the value is dropped.
///
/// # Examples
///
/// ```
/// # use std::net::TcpListener;
/// # use std::error::Error;
/// #
/// # fn main() -> Result<(), Box<dyn Error>> {
/// #     let _listener = TcpListener::bind("127.0.0.1:34254")?;
/// use mio::{Events, Interests, Poll, Token};
/// use mio::net::TcpStream;
/// use std::time::Duration;
///
/// let stream = TcpStream::connect("127.0.0.1:34254".parse()?)?;
///
/// let mut poll = Poll::new()?;
/// let mut events = Events::with_capacity(128);
///
/// // Register the socket with `Poll`
/// poll.registry().register(&stream, Token(0), Interests::WRITABLE)?;
///
/// poll.poll(&mut events, Some(Duration::from_millis(100)))?;
///
/// // The socket might be ready at this point
/// #     Ok(())
/// # }
/// ```
pub struct TcpStream {
    sys: sys::TcpStream,
    #[cfg(debug_assertions)]
    selector_id: SelectorId,
}

use std::net::Shutdown;

impl TcpStream {
    pub(crate) fn new(sys: sys::TcpStream) -> TcpStream {
        TcpStream {
            sys,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        }
    }

    /// Create a new TCP stream and issue a non-blocking connect to the
    /// specified address.
    pub fn connect(addr: SocketAddr) -> io::Result<TcpStream> {
        sys::TcpStream::connect(addr).map(|sys| TcpStream {
            sys,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        })
    }

    /// Returns the socket address of the remote peer of this TCP connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.sys.peer_addr()
    }

    /// Returns the socket address of the local half of this TCP connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sys.local_addr()
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `TcpStream` is a reference to the same stream that this
    /// object references. Both handles will read and write the same stream of
    /// data, and options set on one stream will be propagated to the other
    /// stream.
    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.sys.try_clone().map(|s| TcpStream {
            sys: s,
            #[cfg(debug_assertions)]
            selector_id: self.selector_id.clone(),
        })
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O on the specified
    /// portions to return immediately with an appropriate value (see the
    /// documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.sys.shutdown(how)
    }

    /// Sets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// If set, this option disables the Nagle algorithm. This means that
    /// segments are always sent as soon as possible, even if there is only a
    /// small amount of data. When not set, data is buffered until there is a
    /// sufficient amount to send out, thereby avoiding the frequent sending of
    /// small packets.
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.sys.set_nodelay(nodelay)
    }

    /// Gets the value of the `TCP_NODELAY` option on this socket.
    ///
    /// For more information about this option, see [`set_nodelay`][link].
    ///
    /// [link]: #method.set_nodelay
    pub fn nodelay(&self) -> io::Result<bool> {
        self.sys.nodelay()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.sys.set_ttl(ttl)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`][link].
    ///
    /// [link]: #method.set_ttl
    pub fn ttl(&self) -> io::Result<u32> {
        self.sys.ttl()
    }

    /// Get the value of the `SO_ERROR` option on this socket.
    ///
    /// This will retrieve the stored error in the underlying socket, clearing
    /// the field in the process. This can be useful for checking errors between
    /// calls.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.sys.take_error()
    }

    /// Receives data on the socket from the remote address to which it is
    /// connected, without removing that data from the queue. On success,
    /// returns the number of bytes peeked.
    ///
    /// Successive calls return the same data. This is accomplished by passing
    /// `MSG_PEEK` as a flag to the underlying recv system call.
    pub fn peek(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.sys.peek(buf)
    }
}

impl Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.sys).read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        (&self.sys).read_vectored(bufs)
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.sys).read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        (&self.sys).read_vectored(bufs)
    }
}

impl Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.sys).write(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        (&self.sys).write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.sys).flush()
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.sys).write(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        (&self.sys).write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.sys).flush()
    }
}

impl event::Source for TcpStream {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        #[cfg(debug_assertions)]
        self.selector_id.associate_selector(registry)?;
        self.sys.register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.sys.reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.sys.deregister(registry)
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.sys, f)
    }
}

#[cfg(unix)]
impl IntoRawFd for TcpStream {
    fn into_raw_fd(self) -> RawFd {
        self.sys.into_raw_fd()
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
        TcpStream {
            sys: FromRawFd::from_raw_fd(fd),
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        }
    }
}
