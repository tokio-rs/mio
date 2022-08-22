use std::os::raw::c_int;
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::path::Path;
use std::{fmt, io, mem};

use windows_sys::Win32::Networking::WinSock::{sockaddr_un, AF_UNIX, SOCKET_ERROR};

use super::{socket::Socket, socket_addr, SocketAddr, UnixStream};

/// A Unix domain socket server
///
/// # Examples
///
/// ```no_run
/// use std::thread;
/// use mio::windows::std::net::{UnixStream, UnixListener};
///
/// fn handle_client(stream: UnixStream) {
///     // ...
/// #   drop(stream); // Silence unused variable warning.
/// }
///
/// let listener = UnixListener::bind("/path/to/the/socket").unwrap();
///
/// // accept connections and process them, spawning a new thread for each one
/// for stream in listener.incoming() {
///     match stream {
///         Ok(stream) => {
///             /* connection succeeded */
///             thread::spawn(|| handle_client(stream));
///         }
///         Err(err) => {
///             /* connection failed */
///             eprintln!("connection failed: {err}");
///             break;
///         }
///     }
/// }
/// ```
pub struct UnixListener(Socket);

impl fmt::Debug for UnixListener {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = fmt.debug_struct("UnixListener");
        builder.field("socket", &self.0.as_raw_socket());
        if let Ok(addr) = self.local_addr() {
            builder.field("local", &addr);
        }
        builder.finish()
    }
}

impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixListener;
    ///
    /// let listener = match UnixListener::bind("/path/to/the/socket") {
    ///     Ok(sock) => sock,
    ///     Err(e) => {
    ///         println!("Couldn't connect: {:?}", e);
    ///         return
    ///     }
    /// };
    /// # drop(listener); // Silence unused variable warning.
    /// ```
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        let inner = Socket::new()?;
        let (addr, len) = socket_addr(path.as_ref())?;

        wsa_syscall!(
            bind(
                inner.as_raw_socket() as _,
                &addr as *const _ as *const _,
                len as _,
            ),
            PartialEq::eq,
            SOCKET_ERROR
        )?;
        wsa_syscall!(
            listen(inner.as_raw_socket() as _, 128),
            PartialEq::eq,
            SOCKET_ERROR
        )?;
        Ok(UnixListener(inner))
    }

    /// Accepts a new incoming connection to this listener.
    ///
    /// This function will block the calling thread until a new Unix connection
    /// is established. When established, the corresponding [`UnixStream`] and
    /// the remote peer's address will be returned.
    ///
    /// [`UnixStream`]: struct.UnixStream.html
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixListener;
    ///
    /// let listener = UnixListener::bind("/path/to/the/socket").unwrap();
    ///
    /// match listener.accept() {
    ///     Ok((_socket, addr)) => println!("Got a client: {:?}", addr),
    ///     Err(e) => println!("accept function failed: {:?}", e),
    /// }
    /// ```
    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        let sockaddr = mem::MaybeUninit::<sockaddr_un>::zeroed();

        // This is safe to assume because a `sockaddr_un` filled with `0`
        // bytes is properly initialized.
        //
        // `0` is a valid value for `sockaddr_un::sun_family`; it is
        // `WinSock::AF_UNSPEC`.
        //
        // `[0; 108]` is a valid value for `sockaddr_un::sun_path`; it begins an
        // abstract path.
        let mut sockaddr = unsafe { sockaddr.assume_init() };

        sockaddr.sun_family = AF_UNIX;
        let mut socklen = mem::size_of_val(&sockaddr) as c_int;

        let sock = self
            .0
            .accept(&mut sockaddr as *mut _ as *mut _, &mut socklen)?;
        let addr = SocketAddr::from_parts(sockaddr, socklen);
        Ok((UnixStream(sock), addr))
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `UnixListener` is a reference to the same socket that this
    /// object references. Both handles can be used to accept incoming
    /// connections and options set on one listener will affect the other.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixListener;
    ///
    /// let listener = UnixListener::bind("/path/to/the/socket").unwrap();
    ///
    /// let listener_copy = listener.try_clone().expect("Couldn't clone socket");
    /// # drop(listener_copy); // Silence unused variable warning.
    /// ```
    pub fn try_clone(&self) -> io::Result<UnixListener> {
        self.0.duplicate().map(UnixListener)
    }

    /// Returns the local socket address of this listener.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixListener;
    ///
    /// let listener = UnixListener::bind("/path/to/the/socket").unwrap();
    ///
    /// let addr = listener.local_addr().expect("Couldn't get local address");
    /// # drop(addr); // Silence unused variable warning.
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|addr, len| {
            wsa_syscall!(
                getsockname(self.0.as_raw_socket() as _, addr, len),
                PartialEq::eq,
                SOCKET_ERROR
            )
        })
    }

    /// Moves the socket into or out of nonblocking mode.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixListener;
    ///
    /// let listener = UnixListener::bind("/path/to/the/socket").unwrap();
    ///
    /// listener.set_nonblocking(true).expect("Couldn't set nonblocking");
    /// ```
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }

    /// Returns the value of the `SO_ERROR` option.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixListener;
    ///
    /// let listener = UnixListener::bind("/tmp/sock").unwrap();
    ///
    /// if let Ok(Some(err)) = listener.take_error() {
    ///     println!("Got error: {:?}", err);
    /// }
    /// ```
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.take_error()
    }

    /// Returns an iterator over incoming connections.
    ///
    /// The iterator will never return `None` and will also not yield the
    /// peer's [`SocketAddr`] structure.
    ///
    /// [`SocketAddr`]: struct.SocketAddr.html
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::thread;
    /// use mio::windows::std::net::{UnixStream, UnixListener};
    ///
    /// fn handle_client(stream: UnixStream) {
    ///     // ...
    /// #   drop(stream); // Silence unused variable warning.
    /// }
    ///
    /// let listener = UnixListener::bind("/path/to/the/socket").unwrap();
    ///
    /// for stream in listener.incoming() {
    ///     match stream {
    ///         Ok(stream) => {
    ///             thread::spawn(|| handle_client(stream));
    ///         }
    ///         Err(err) => {
    ///             eprintln!("connection failed: {err}");
    ///             break;
    ///         }
    ///     }
    /// }
    /// ```
    pub fn incoming(&self) -> Incoming<'_> {
        Incoming { listener: self }
    }
}

impl AsRawSocket for UnixListener {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}

impl FromRawSocket for UnixListener {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        UnixListener(Socket::from_raw_socket(sock))
    }
}

impl IntoRawSocket for UnixListener {
    fn into_raw_socket(self) -> RawSocket {
        let ret = self.0.as_raw_socket();
        mem::forget(self);
        ret
    }
}

impl<'a> IntoIterator for &'a UnixListener {
    type Item = io::Result<UnixStream>;
    type IntoIter = Incoming<'a>;

    fn into_iter(self) -> Incoming<'a> {
        self.incoming()
    }
}

/// An iterator over incoming connections to a [`UnixListener`].
///
/// It will never return `None`.
///
/// [`UnixListener`]: struct.UnixListener.html
///
/// # Examples
///
/// ```no_run
/// use std::thread;
/// use mio::windows::std::net::{UnixStream, UnixListener};
///
/// fn handle_client(stream: UnixStream) {
///     // ...
/// #   drop(stream); // Silence unused variable warning.
/// }
///
/// let listener = UnixListener::bind("/path/to/the/socket").unwrap();
///
/// for stream in listener.incoming() {
///     match stream {
///         Ok(stream) => {
///             thread::spawn(|| handle_client(stream));
///         }
///         Err(err) => {
///             eprintln!("connection failed: {err}");
///             break;
///         }
///     }
/// }
/// ```
#[derive(Debug)]
pub struct Incoming<'a> {
    listener: &'a UnixListener,
}

impl<'a> Iterator for Incoming<'a> {
    type Item = io::Result<UnixStream>;

    fn next(&mut self) -> Option<io::Result<UnixStream>> {
        Some(self.listener.accept().map(|s| s.0))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::max_value(), None)
    }
}
