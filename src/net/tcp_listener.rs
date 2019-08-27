use super::TcpStream;
#[cfg(debug_assertions)]
use crate::poll::SelectorId;
use crate::{event, sys, Interests, Registry, Token};

use std::fmt;
use std::io;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

/// A structure representing a socket server
///
/// # Examples
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// use mio::{Events, Interests, Poll, Token};
/// use mio::net::TcpListener;
/// use std::time::Duration;
///
/// let listener = TcpListener::bind("127.0.0.1:34255".parse()?)?;
///
/// let mut poll = Poll::new()?;
/// let mut events = Events::with_capacity(128);
///
/// // Register the socket with `Poll`
/// poll.registry().register(&listener, Token(0), Interests::READABLE)?;
///
/// poll.poll(&mut events, Some(Duration::from_millis(100)))?;
///
/// // There may be a socket ready to be accepted
/// #     Ok(())
/// # }
/// ```
pub struct TcpListener {
    sys: sys::TcpListener,
    #[cfg(debug_assertions)]
    selector_id: SelectorId,
}

impl TcpListener {
    /// Convenience method to bind a new TCP listener to the specified address
    /// to receive new connections.
    ///
    /// This function will take the following steps:
    ///
    /// 1. Create a new TCP socket.
    /// 2. Set the `SO_REUSEADDR` option on the socket on Unix.
    /// 3. Bind the socket to the specified address.
    /// 4. Calls `listen` on the socket to prepare it to receive new connections.
    pub fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
        sys::TcpListener::bind(addr).map(|sys| TcpListener {
            sys,
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        })
    }

    /// Accepts a new `TcpStream`.
    ///
    /// This may return an `Err(e)` where `e.kind()` is
    /// `io::ErrorKind::WouldBlock`. This means a stream may be ready at a later
    /// point and one should wait for an event before calling `accept` again.
    ///
    /// If an accepted stream is returned, the remote address of the peer is
    /// returned along with it.
    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        self.sys
            .accept()
            .map(|(sys, addr)| (TcpStream::new(sys), addr))
    }

    /// Returns the local socket address of this listener.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sys.local_addr()
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `TcpListener` is a reference to the same socket that this
    /// object references. Both handles can be used to accept incoming
    /// connections and options set on one listener will affect the other.
    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.sys.try_clone().map(|s| TcpListener {
            sys: s,
            #[cfg(debug_assertions)]
            selector_id: self.selector_id.clone(),
        })
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
}

impl event::Source for TcpListener {
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

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.sys, f)
    }
}

#[cfg(unix)]
impl IntoRawFd for TcpListener {
    fn into_raw_fd(self) -> RawFd {
        self.sys.into_raw_fd()
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
        TcpListener {
            sys: FromRawFd::from_raw_fd(fd),
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        }
    }
}
