use super::{socket::Socket, SocketAddr};
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::{fmt, io, mem};
use windows_sys::Win32::Networking::WinSock::SOCKET_ERROR;

/// A structure representing a Unix domain socket server.
pub struct UnixListener {
    inner: Socket,
}

impl UnixListener {
    /// Returns the local socket address of this listener.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|addr, len| {
            wsa_syscall!(
                getsockname(self.inner.as_raw_socket() as _, addr, len),
                SOCKET_ERROR
            )
        })
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }
}

cfg_os_poll! {
    use std::os::raw::c_int;
    use std::path::Path;
    use super::{socket_addr, UnixStream};
    use windows_sys::Win32::Networking::WinSock::SOCKADDR_UN;

    impl UnixListener {
        /// Creates a new `UnixListener` bound to the specified socket.
        pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
            let inner = Socket::new()?;
            let (addr, len) = socket_addr(path.as_ref())?;

            wsa_syscall!(
                bind(inner.as_raw_socket() as _, &addr as *const _ as *const _, len as _),
                SOCKET_ERROR
            )?;
            wsa_syscall!(listen(inner.as_raw_socket() as _, 1024), SOCKET_ERROR)?;
            Ok(UnixListener {
                inner
            })
        }

        /// Creates a new `UnixListener` bound to the specified address.
        pub fn bind_addr(socket_addr: &SocketAddr) -> io::Result<UnixListener> {
            let inner = Socket::new()?;

            wsa_syscall!(
                bind(inner.as_raw_socket() as _, &socket_addr.raw_sockaddr() as *const _ as *const _, socket_addr.raw_socklen() as _),
                SOCKET_ERROR
            )?;
            wsa_syscall!(listen(inner.as_raw_socket() as _, 1024), SOCKET_ERROR)?;
            Ok(UnixListener {
                inner
            })
        }

        /// Accepts a new incoming connection to this listener.
        ///
        /// This function will block the calling thread until a new Unix connection
        /// is established. When established, the corresponding [`UnixStream`] and
        /// the remote peer's address will be returned.
        pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
            let mut storage: SOCKADDR_UN = unsafe { mem::zeroed() };
            let mut len = mem::size_of_val(&storage) as c_int;
            let sock = self.inner.accept(&mut storage as *mut _ as *mut _, &mut len)?;
            let addr = SocketAddr::from_parts(storage, len)?;
            Ok((UnixStream(sock), addr))
        }

        /// Moves the socket into or out of nonblocking mode.
        ///
        /// This will result in the `accept` operation becoming nonblocking,
        /// i.e., immediately returning from their calls. If the IO operation is
        /// successful, `Ok` is returned and no further action is required. If the
        /// IO operation could not be completed and needs to be retried, an error
        /// with kind [`io::ErrorKind::WouldBlock`] is returned.
        pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
            self.inner.set_nonblocking(nonblocking)
        }
    }
}

impl fmt::Debug for UnixListener {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = fmt.debug_struct("UnixListener");
        builder.field("socket", &self.inner.as_raw_socket());
        if let Ok(addr) = self.local_addr() {
            builder.field("local", &addr);
        }
        builder.finish()
    }
}

impl AsRawSocket for UnixListener {
    fn as_raw_socket(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

impl FromRawSocket for UnixListener {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        UnixListener {
            inner: Socket::from_raw_socket(sock),
        }
    }
}

impl IntoRawSocket for UnixListener {
    fn into_raw_socket(self) -> RawSocket {
        let ret = self.inner.as_raw_socket();
        mem::forget(self);
        ret
    }
}
