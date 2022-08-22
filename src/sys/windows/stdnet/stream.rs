use std::convert::TryInto;
use std::io::{self, IoSlice, IoSliceMut};
use std::net::Shutdown;
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fmt, mem};

use windows_sys::Win32::Networking::WinSock::{
    SOCKET_ERROR, SO_RCVTIMEO, SO_SNDTIMEO, WSAEINPROGRESS,
};
use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom,
    BCRYPT_USE_SYSTEM_PREFERRED_RNG
};
use windows_sys::Win32::Foundation::STATUS_SUCCESS;

use super::{socket::Socket, socket_addr, SocketAddr, UnixListener};

/// A Unix stream socket
///
/// # Examples
///
/// ```no_run
/// use mio::windows::std::net::UnixStream;
/// use std::io::prelude::*;
///
/// let mut stream = UnixStream::connect("/path/to/my/socket").unwrap();
/// stream.write_all(b"hello world").unwrap();
/// let mut response = String::new();
/// stream.read_to_string(&mut response).unwrap();
/// println!("{}", response);
/// ```
pub struct UnixStream(pub(super) Socket);

impl fmt::Debug for UnixStream {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = fmt.debug_struct("UnixStream");
        builder.field("socket", &self.0.as_raw_socket());
        if let Ok(addr) = self.local_addr() {
            builder.field("local", &addr);
        }
        if let Ok(addr) = self.peer_addr() {
            builder.field("peer", &addr);
        }
        builder.finish()
    }
}

impl UnixStream {
    /// Connects to the socket named by `path`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = match UnixStream::connect("/tmp/sock") {
    ///     Ok(sock) => sock,
    ///     Err(e) => {
    ///         println!("Couldn't connect: {:?}", e);
    ///         return
    ///     }
    /// };
    /// # drop(socket); // Silence unused variable warning.
    /// ```
    pub fn connect<P: AsRef<Path>>(path: P) -> io::Result<UnixStream> {
        let inner = Socket::new()?;
        let (addr, len) = socket_addr(path.as_ref())?;

        match wsa_syscall!(
            connect(
                inner.as_raw_socket() as _,
                &addr as *const _ as *const _,
                len as i32,
            ),
            PartialEq::eq,
            SOCKET_ERROR
        ) {
            Ok(_) => {}
            Err(ref err) if err.raw_os_error() == Some(WSAEINPROGRESS) => {}
            Err(e) => return Err(e),
        }
        Ok(UnixStream(inner))
    }

    /// Creates a new independently owned handle to the underlying socket.
    ///
    /// The returned `UnixStream` is a reference to the same stream that this
    /// object references. Both handles will read and write the same stream of
    /// data, and options set on one stream will be propagated to the other
    /// stream.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// let sock_copy = socket.try_clone().expect("Couldn't clone socket");
    /// # drop(sock_copy); // Silence unused variable warning.
    /// ```
    pub fn try_clone(&self) -> io::Result<UnixStream> {
        self.0.duplicate().map(UnixStream)
    }

    /// Returns the socket address of the local half of this connection.
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// let addr = socket.local_addr().expect("Couldn't get local address");
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

    /// Returns the socket address of the remote half of this connection.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// let addr = socket.peer_addr().expect("Couldn't get peer address");
    /// # drop(addr); // Silence unused variable warning.
    /// ```
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|addr, len| {
            wsa_syscall!(
                getpeername(self.0.as_raw_socket() as _, addr, len),
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
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// socket.set_nonblocking(true).expect("Couldn't set nonblocking");
    /// ```
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }

    /// Returns the value of the `SO_ERROR` option.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// if let Ok(Some(err)) = socket.take_error() {
    ///     println!("Got error: {:?}", err);
    /// }
    /// ```
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.take_error()
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation for `Shutdown`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    /// use std::net::Shutdown;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// socket.shutdown(Shutdown::Both).expect("shutdown function failed");
    /// ```
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.0.shutdown(how)
    }

    /// Creates an unnamed pair of connected sockets.
    ///
    /// Returns two `UnixStream`s which are connected to each other.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let (sock1, sock2) = match UnixStream::pair() {
    ///     Ok((sock1, sock2)) => (sock1, sock2),
    ///     Err(e) => {
    ///         println!("Couldn't create a pair of sockets: {e:?}");
    ///         return
    ///     }
    /// };
    /// # drop(sock1); // Silence unused variable warning.
    /// # drop(sock2); // Silence unused variable warning.
    /// ```
    pub fn pair() -> io::Result<(Self, Self)> {
        use std::sync::{Arc, RwLock};
        use std::thread::spawn;

        let file_path = TempPath::new(10)?;
        let a: Arc<RwLock<Option<io::Result<UnixStream>>>> = Arc::new(RwLock::new(None));
        let ul = UnixListener::bind(&file_path).unwrap();
        let server = {
            let a = a.clone();
            spawn(move || {
                let mut store = a.write().unwrap();
                let stream0 = ul.accept().map(|s| s.0);
                *store = Some(stream0);
            })
        };
        let stream1 = UnixStream::connect(&file_path)?;
        server
            .join()
            .map_err(|_| io::Error::from(io::ErrorKind::ConnectionRefused))?;
        let stream0 = (*(a.write().unwrap())).take().unwrap()?;
        Ok((stream0, stream1))
    }

    /// Sets the read timeout to the timeout specified.
    ///
    /// If the value specified is `None`, then `read` calls will block
    /// indefinitely. An `Err` is returned if the zero `Duration` is
    /// passed to this method.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// socket.set_read_timeout(None).expect("Couldn't set read timeout");
    /// ```
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.0.set_timeout(dur, SO_RCVTIMEO.try_into().unwrap())
    }

    /// Sets the write timeout to the timeout specified.
    ///
    /// If the value specified is `None`, then `write` calls will block
    /// indefinitely. An `Err` is returned if the zero `Duration` is
    /// passed to this method.
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// socket.set_write_timeout(None).expect("Couldn't set write timeout");
    /// ```
    pub fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.0.set_timeout(dur, SO_SNDTIMEO.try_into().unwrap())
    }

    /// Returns the read timeout of this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// socket.set_read_timeout(None).expect("Couldn't set read timeout");
    /// assert_eq!(socket.read_timeout().unwrap(), None);
    /// ```
    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        self.0.timeout(SO_RCVTIMEO.try_into().unwrap())
    }

    /// Returns the write timeout of this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mio::windows::std::net::UnixStream;
    ///
    /// let socket = UnixStream::connect("/tmp/sock").unwrap();
    /// socket.set_write_timeout(None).expect("Couldn't set write timeout");
    /// assert_eq!(socket.write_timeout().unwrap(), None);
    /// ```
    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        self.0.timeout(SO_SNDTIMEO.try_into().unwrap())
    }
}

impl io::Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut &*self, buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        io::Read::read_vectored(&mut &*self, bufs)
    }
}

impl<'a> io::Read for &'a UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.0.recv_vectored(bufs)
    }
}

impl io::Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut &*self, buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        io::Write::write_vectored(&mut &*self, bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut &*self)
    }
}

impl<'a> io::Write for &'a UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.send(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        self.0.send_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsRawSocket for UnixStream {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}

impl FromRawSocket for UnixStream {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        UnixStream(Socket::from_raw_socket(sock))
    }
}

impl IntoRawSocket for UnixStream {
    fn into_raw_socket(self) -> RawSocket {
        let ret = self.0.as_raw_socket();
        mem::forget(self);
        ret
    }
}

struct TempPath(PathBuf);

fn sample_ascii_string(len: usize) -> io::Result<String> {
    const GEN_ASCII_STR_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
            abcdefghijklmnopqrstuvwxyz\
            0123456789-_";
    let mut result = String::with_capacity(len);
    let mut buf = [0; 4];
    for _ in 0..len {
        syscall!(
            BCryptGenRandom(
                0,
                &mut buf as *mut _,
                buf.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            ),
            PartialEq::ne,
            STATUS_SUCCESS
        )?;
        // We pick from 64=2^6 characters so we can use a simple bitshift.
        let var = u32::from_le_bytes(buf) >> (32 - 6);
        result.push(char::from(GEN_ASCII_STR_CHARSET[var as usize]));
    }
    Ok(result)
}

impl TempPath {
    fn new(random_len: usize) -> io::Result<Self> {
        let dir = std::env::temp_dir();
        // Retry a few times in case of collisions
        for _ in 0..10 {
            let rand_str = sample_ascii_string(random_len)?;
            let filename = format!(".tmp-{rand_str}.socket");
            let path = dir.join(filename);
            if !path.exists() {
                return Ok(Self(path));
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "too many temporary files exist",
        ))
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl AsRef<Path> for TempPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for TempPath {
    type Target = Path;
    fn deref(&self) -> &Path {
        Path::new(&self.0)
    }
}
