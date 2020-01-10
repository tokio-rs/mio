use crate::sys::windows::net::from_socket_addr;

use std::io::{self, Result};
use std::net::SocketAddr;
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};
use std::os::windows::raw::SOCKET as StdSocket; // winapi uses usize, stdlib uses u32/u64.

use winapi::ctypes::c_int;
use winapi::um::winsock2::{
    bind, closesocket, ioctlsocket, socket, FIONBIO, INVALID_SOCKET, PF_INET, PF_INET6, SOCKET,
    SOCKET_ERROR,
};
#[cfg(feature = "tcp")]
use winapi::um::winsock2::{connect, listen};

pub(crate) struct Socket {
    socket: SOCKET,
}

impl Socket {
    pub(crate) fn new(af: c_int, socket_type: c_int, protocol: c_int) -> Result<Self> {
        let socket = syscall!(
            socket(af, socket_type, protocol),
            PartialEq::eq,
            INVALID_SOCKET
        )?;
        // Set the socket I/O mode: FIOBIO enables non-blocking based on the
        // numerical value of iMode (1).
        syscall!(ioctlsocket(socket, FIONBIO, &mut 1), PartialEq::ne, 0)?;
        Ok(unsafe { Socket::from_raw_socket(socket as StdSocket) })
    }

    pub(crate) fn from_addr(addr: SocketAddr, socket_type: c_int, protocol: c_int) -> Result<Self> {
        let af = match addr {
            SocketAddr::V4(..) => PF_INET,
            SocketAddr::V6(..) => PF_INET6,
        };
        Self::new(af, socket_type, protocol)
    }

    #[cfg(feature = "tcp")]
    pub(crate) fn connect(&self, addr: SocketAddr) -> Result<i32> {
        let (storage, len) = from_socket_addr(&addr);
        match syscall!(
            connect(self.socket, storage, len),
            PartialEq::eq,
            SOCKET_ERROR
        ) {
            Ok(res) => Ok(res),
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => Ok(0),
            Err(err) => {
                // Close the socket if we hit an error, ignoring the error
                // from closing since we can't pass back two errors.
                let _ = unsafe { closesocket(self.socket) };
                Err(err)
            }
        }
    }

    pub(crate) fn bind(&self, addr: SocketAddr) -> Result<i32> {
        let (storage, len) = from_socket_addr(&addr);
        syscall!(bind(self.socket, storage, len), PartialEq::eq, SOCKET_ERROR).map_err(|err| {
            // Close the socket if we hit an error, ignoring the error from
            // closing since we can't pass back two errors.
            let _ = unsafe { closesocket(self.socket) };
            err
        })
    }

    #[cfg(feature = "tcp")]
    pub(crate) fn listen(&self, backlog: i32) -> Result<i32> {
        syscall!(listen(self.socket, backlog), PartialEq::eq, SOCKET_ERROR)
    }
}

impl AsRawSocket for Socket {
    fn as_raw_socket(&self) -> RawSocket {
        self.socket as StdSocket
    }
}

impl FromRawSocket for Socket {
    unsafe fn from_raw_socket(socket: RawSocket) -> Self {
        Socket {
            socket: socket as SOCKET,
        }
    }
}

impl IntoRawSocket for Socket {
    fn into_raw_socket(self) -> RawSocket {
        self.socket as StdSocket
    }
}
