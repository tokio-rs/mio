use super::TcpStream;
#[cfg(debug_assertions)]
use crate::poll::SelectorId;
use crate::{event, sys, Interests, Registry, Token};

use std::fmt;
use std::io;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::raw::SOCKET as RawFd;

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
        TcpListener::bind_with_options(addr, |_| Ok(()))
    }

    /// Bind a new TCP listener to the address, calling `set_opts` before the
    /// call to bind allowing for user defined modifications of the socket. If
    /// the provided `set_opts` does nothing this does the same as
    /// `TcpListener::bind`.
    ///
    /// The socket provided to `set_opts` may not be closed. Any error returned
    /// by the `set_opts` function will be seen as fatal for the creation of
    /// `TcpListener` and will be returned.
    ///
    /// # Notes
    ///
    /// The provided `set_opts` function is different depending on the OS. On
    /// Unix platforms it accepts a [`RawFd`], on Windows [`SOCKET`].
    ///
    /// [`RawFd`]: std::os::unix::io::RawFd
    /// [`SOCKET`]: std::os::windows::raw::SOCKET
    ///
    /// # Examples
    ///
    /// Enabling `SO_REUSEPORT`.
    ///
    #[cfg_attr(unix, doc = " ```")]
    #[cfg_attr(not(unix), doc = " ```no_run")]
    /// use std::io;
    /// use std::mem::size_of_val;
    /// use std::os::unix::io::RawFd;
    /// use std::time::Duration;
    ///
    /// use libc;
    /// use mio::net::{TcpStream, TcpListener};
    /// use mio::{Events, Poll, Interests, Token};
    ///
    /// const LISTENER1: Token = Token(0);
    /// const LISTENER2: Token = Token(1);
    /// const STREAM: Token = Token(2);
    ///
    /// /// Sets `SO_REUSEPORT` to 1 on `socket`.
    /// fn enable_reuse_port(socket: RawFd) -> io::Result<()> {
    ///     let enable: libc::c_int = 1;
    ///     let ok = unsafe {
    ///         libc::setsockopt(socket, libc::SOL_SOCKET, libc::SO_REUSEPORT,
    ///             (&enable as *const libc::c_int) as *const libc::c_void,
    ///             size_of_val(&enable) as libc::socklen_t)
    ///     };
    ///     if ok == 0 {
    ///         Ok(())
    ///     } else {
    ///         Err(io::Error::last_os_error())
    ///     }
    /// }
    ///
    /// fn main() -> io::Result<()> {
    ///     let mut poll = Poll::new()?;
    ///     let mut events = Events::with_capacity(16);
    ///
    ///     // Using the `SO_REUSEPORT` we can bind two listeners to the same
    ///     // address.
    ///     let addr = "127.0.0.1:9191".parse().unwrap();
    ///     let listener1 = TcpListener::bind_with_options(addr, enable_reuse_port)?;
    ///     let listener2 = TcpListener::bind_with_options(addr, enable_reuse_port)?;
    ///
    ///     poll.registry().register(&listener1, LISTENER1, Interests::READABLE)?;
    ///     poll.registry().register(&listener1, LISTENER2, Interests::READABLE)?;
    ///
    ///     let stream = TcpStream::connect(addr)?;
    ///     poll.registry().register(&stream, STREAM, Interests::WRITABLE)?;
    ///
    ///     poll.poll(&mut events, Some(Duration::from_millis(100)))?;
    ///     // We should have at least one event now.
    ///     assert!(!events.is_empty());
    ///
    /// #   // Silence unused warnings.
    /// #   drop((listener1, listener2, stream));
    ///     Ok(())
    /// }
    /// ```
    #[cfg(any(unix, windows))]
    pub fn bind_with_options<F>(addr: SocketAddr, set_opts: F) -> io::Result<TcpListener>
    where
        F: FnOnce(RawFd) -> io::Result<()>,
    {
        sys::TcpListener::bind_with_options(addr, set_opts).map(|sys| TcpListener {
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
