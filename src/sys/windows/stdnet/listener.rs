use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::{fmt, io, mem};

use windows_sys::Win32::Networking::WinSock::SOCKET_ERROR;

use super::{socket::Socket, SocketAddr};

pub(crate) struct UnixListener(Socket);

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
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|addr, len| {
            wsa_syscall!(
                getsockname(self.0.as_raw_socket() as _, addr, len),
                SOCKET_ERROR
            )
        })
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.0.take_error()
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

cfg_os_poll! {
use std::path::Path;

use super::{socket_addr, UnixStream};

impl UnixListener {
    pub fn bind<P: AsRef<Path>>(path: P) -> io::Result<UnixListener> {
        let inner = Socket::new()?;
        let (addr, len) = socket_addr(path.as_ref())?;

        wsa_syscall!(
            bind(inner.as_raw_socket() as _, &addr as *const _ as *const _, len as _),
            SOCKET_ERROR
        )?;
        wsa_syscall!(listen(inner.as_raw_socket() as _, 128), SOCKET_ERROR)?;
        Ok(UnixListener(inner))
    }

    pub fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
        SocketAddr::init(|addr, len| self.0.accept(addr, len))
            .map(|(sock, addr)| (UnixStream(sock), addr))
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }
}
}
