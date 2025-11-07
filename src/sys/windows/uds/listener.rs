use super::{socketaddr_un, startup, wsa_error, Socket, SocketAddr, UnixStream};
use std::{
    io,
    os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket},
    path::Path,
};
use windows_sys::Win32::Networking::WinSock::{self, SOCKADDR_UN, SOCKET_ERROR};
/// A Unix domain socket server for listening to incoming connections.
///
/// This structure represents a socket server that listens for incoming Unix domain socket
/// connections on Windows systems. After creating a `UnixListener` by binding it to a socket
/// address, it can accept incoming connections from clients.
///
/// The `UnixListener` wraps an underlying `Socket` and provides a higher-level interface
/// for server-side Unix domain socket operations.
///
/// # Examples
///
/// ```no_run
/// use std::io;
/// use mio::sys::uds::UnixListener;
///
/// fn main() -> io::Result<()> {
///     // Bind to a socket file
///     let listener = UnixListener::bind("/tmp/socket.sock")?;
///     
///     // Accept incoming connections
///     match listener.accept() {
///         Ok((stream, addr)) => {
///             println!("New connection from {:?}", addr);
///             // Handle the connection with the stream...
///         }
///         Err(e) => eprintln!("Connection failed: {}", e),
///     }
///     
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct UnixListener(Socket);

impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified path.
    ///
    /// This function will perform the following operations:
    /// 1. Initialize the Winsock library
    /// 2. Create a new socket
    /// 3. Convert the provided path to a socket address
    /// 4. Bind the socket to the address
    /// 5. Start listening for incoming connections with a backlog of 5
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path to bind the socket to
    ///
    /// # Errors
    ///
    /// This function will return an error in the following situations:
    ///
    /// * Winsock initialization fails
    /// * Socket creation fails
    /// * The path cannot be converted to a valid socket address
    /// * Binding to the specified path fails
    /// * Listening on the socket fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::sys::uds::UnixListener;
    ///
    /// let listener = UnixListener::bind("/tmp/socket.sock").unwrap();
    /// ```
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        unsafe {
            startup()?;
            let s = Socket::new()?;
            let (addr, len) = socketaddr_un(path.as_ref())?;
            if WinSock::bind(s.0, &addr as *const _ as *const _, len) == SOCKET_ERROR {
                Err(wsa_error())
            } else {
                match WinSock::listen(s.0, 5) {
                    SOCKET_ERROR => Err(wsa_error()),
                    _ => Ok(Self(s)),
                }
            }
        }
    }

    /// Creates a new `UnixListener` bound to the specified socket address.
    ///
    /// This function allows binding to a pre-constructed `SocketAddr` instead of
    /// creating one from a path. This can be useful when you need more control
    /// over the socket address configuration or when reusing addresses.
    ///
    /// Unlike `bind`, this function does not initialize Winsock, assuming it has
    /// already been initialized elsewhere.
    ///
    /// # Arguments
    ///
    /// * `socket_addr` - The socket address to bind to
    ///
    /// # Errors
    ///
    /// This function will return an error in the following situations:
    ///
    /// * Socket creation fails
    /// * Binding to the specified address fails
    /// * Listening on the socket fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::sys::uds::{UnixListener, SocketAddr};
    /// use std::path::Path;
    ///
    /// // Create a socket address first
    /// let addr = SocketAddr::from_path(Path::new("/tmp/socket.sock")).unwrap();
    /// let listener = UnixListener::bind_addr(&addr).unwrap();
    /// ```
    pub fn bind_addr(socket_addr: &SocketAddr) -> io::Result<Self> {
        unsafe {
            let s = Socket::new()?;
            if WinSock::bind(
                s.0,
                &socket_addr.addr as *const _ as *const _,
                socket_addr.addrlen,
            ) == SOCKET_ERROR
            {
                Err(wsa_error())
            } else {
                match WinSock::listen(s.0, 5) {
                    SOCKET_ERROR => Err(wsa_error()),
                    _ => Ok(Self(s)),
                }
            }
        }
    }

    /// Accepts a new incoming connection to this listener.
    ///
    /// This function will block the calling thread until a new Unix domain socket
    /// connection is established. When established, the corresponding [`UnixStream`]
    /// and the remote peer's address will be returned.
    ///
    /// The returned [`UnixStream`] can be used to read and write data to the connected
    /// client, while the [`SocketAddr`] contains information about the client's address.
    ///
    /// # Errors
    ///
    /// This function will return an error if the underlying socket call fails.
    /// Specific errors may include:
    ///
    /// * The socket is not bound or listening
    /// * The socket has been closed
    /// * Insufficient resources to complete the operation
    /// * The operation was interrupted
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use your_crate::UnixListener;
    ///
    /// let listener = UnixListener::bind("/tmp/socket.sock").unwrap();
    ///
    /// // Accept connections in a loop
    /// for stream_result in listener.incoming() {
    ///     match stream_result {
    ///         Ok((stream, addr)) => {
    ///             println!("New connection from {:?}", addr);
    ///             // Handle the connection...
    ///         }
    ///         Err(e) => {
    ///             eprintln!("Accept error: {}", e);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        let mut addr = SOCKADDR_UN::default();
        let mut addrlen = size_of::<SOCKADDR_UN>() as _;
        let s = self
            .0
            .accept(&mut addr as *mut _ as *mut _, &mut addrlen as *mut _)?;
        Ok((UnixStream::new(s), SocketAddr { addr, addrlen }))
    }
    /// Returns the socket address of the local half of this connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }
    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.take_error()
    }
    /// Sets the non-blocking mode for this socket
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }
}

pub(crate)  fn bind_addr(socket_addr: &SocketAddr) -> io::Result<UnixListener> {
    UnixListener::bind_addr(socket_addr)
}
pub(crate) fn accept(s: &UnixListener) -> io::Result<(crate::net::UnixStream, SocketAddr)> {
    let (inner, addr) = s.accept()?;
    Ok((crate::net::UnixStream::from_std(inner), addr))
}

impl AsRawSocket for UnixListener {
    fn as_raw_socket(&self) -> RawSocket {
        self.0 .0 as _
    }
}
impl FromRawSocket for UnixListener {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        Self(Socket(sock as _))
    }
}
impl IntoRawSocket for UnixListener {
    fn into_raw_socket(self) -> RawSocket {
        self.0 .0 as _
    }
}