//! AF_UNIX support for Windows.
//!
//! Provides a `SocketAddr` type (stand-in for the still-unstable
//! `std::os::windows::net::SocketAddr`) and a `Socket` newtype around
//! `OwnedSocket` that implements `Read`/`Write`/`AsRawSocket` so it can be
//! wrapped in `IoSource`. The per-type entry points (`bind_addr`,
//! `connect_addr`, `accept`) live in the `listener` and `stream` submodules,
//! mirroring the layout of `sys::unix::uds`.

use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, OwnedSocket, RawSocket};
use std::path::Path;
use std::{fmt, io, mem, ptr};

use windows_sys::Win32::Networking::WinSock::{
    self as ws, AF_UNIX, SOCKADDR, SOCKADDR_UN, SOCKET_ERROR, SOCK_STREAM, WSABUF,
};

use crate::sys::windows::net;

pub(crate) mod listener;
pub(crate) mod stream;

/// An address associated with a Unix domain socket.
///
/// On Windows this is a stand-in for `std::os::windows::net::SocketAddr`,
/// which is still gated behind the unstable `windows_unix_domain_sockets`
/// feature.
#[derive(Clone, Copy)]
pub struct SocketAddr {
    pub(super) addr: SOCKADDR_UN,
    pub(super) len: i32,
}

impl SocketAddr {
    /// Constructs a `SocketAddr` with the family `AF_UNIX` and the provided
    /// path.
    pub fn from_pathname<P: AsRef<Path>>(path: P) -> io::Result<SocketAddr> {
        let (addr, len) = sockaddr_un(path.as_ref())?;
        Ok(SocketAddr { addr, len })
    }

    /// Returns the contents of this address if it is a pathname address.
    pub fn as_pathname(&self) -> Option<&Path> {
        let path_len = self.len as usize - SUN_PATH_OFFSET;
        if path_len == 0 || self.addr.sun_path[0] == 0 {
            return None;
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(self.addr.sun_path.as_ptr().cast(), path_len) };
        // Truncate at the first null byte (Windows may return the full
        // `sun_path` buffer).
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let s = std::str::from_utf8(&bytes[..end]).ok()?;
        Some(Path::new(s))
    }

    /// Returns `true` if the address is unnamed.
    pub fn is_unnamed(&self) -> bool {
        // Not actually supported at time of writing, but worth being defensive.
        let path_len = self.len as usize - SUN_PATH_OFFSET;
        path_len == 0
    }

    pub(super) fn from_parts(addr: SOCKADDR_UN, len: i32) -> io::Result<SocketAddr> {
        if addr.sun_family != AF_UNIX {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid address family",
            ))
        } else if (len as usize) < SUN_PATH_OFFSET || (len as usize) > mem::size_of::<SOCKADDR_UN>()
        {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid address length",
            ))
        } else {
            Ok(SocketAddr { addr, len })
        }
    }

    fn from_raw(f: impl FnOnce(*mut SOCKADDR, *mut i32) -> i32) -> io::Result<SocketAddr> {
        unsafe {
            let mut addr: SOCKADDR_UN = mem::zeroed();
            let mut len = mem::size_of::<SOCKADDR_UN>() as i32;
            cvt(f(&mut addr as *mut SOCKADDR_UN as *mut SOCKADDR, &mut len))?;
            SocketAddr::from_parts(addr, len)
        }
    }
}

impl fmt::Debug for SocketAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = self.as_pathname() {
            write!(f, "{path:?} (pathname)")
        } else if self.is_unnamed() {
            write!(f, "(unnamed)")
        } else {
            write!(f, "(abstract)")
        }
    }
}

/// Newtype around `OwnedSocket` that implements `Read`, `Write`, and
/// `AsRawSocket` so it can be wrapped in `IoSource`.
pub(crate) struct Socket {
    inner: OwnedSocket,
}

impl Socket {
    pub(crate) fn local_addr(&self) -> io::Result<SocketAddr> {
        let raw = self.inner.as_raw_socket();
        SocketAddr::from_raw(|addr, len| unsafe { ws::getsockname(raw as _, addr, len) })
    }

    pub(crate) fn peer_addr(&self) -> io::Result<SocketAddr> {
        let raw = self.inner.as_raw_socket();
        SocketAddr::from_raw(|addr, len| unsafe { ws::getpeername(raw as _, addr, len) })
    }

    pub(crate) fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        let how = match how {
            std::net::Shutdown::Read => ws::SD_RECEIVE,
            std::net::Shutdown::Write => ws::SD_SEND,
            std::net::Shutdown::Both => ws::SD_BOTH,
        };
        cvt(unsafe { ws::shutdown(self.inner.as_raw_socket() as _, how as _) })?;
        Ok(())
    }

    pub(crate) fn take_error(&self) -> io::Result<Option<io::Error>> {
        let mut optval: i32 = 0;
        let mut optlen = mem::size_of::<i32>() as i32;
        cvt(unsafe {
            ws::getsockopt(
                self.inner.as_raw_socket() as _,
                ws::SOL_SOCKET,
                ws::SO_ERROR,
                &mut optval as *mut _ as *mut _,
                &mut optlen,
            )
        })?;
        if optval == 0 {
            Ok(None)
        } else {
            Ok(Some(io::Error::from_raw_os_error(optval)))
        }
    }
}

impl io::Read for Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self).read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        (&*self).read_vectored(bufs)
    }
}

impl io::Read for &Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let res = unsafe {
            ws::recv(
                self.inner.as_raw_socket() as _,
                buf.as_mut_ptr(),
                buf.len() as i32,
                0,
            )
        };
        if res == SOCKET_ERROR {
            let err = unsafe { ws::WSAGetLastError() };
            // Map shutdown to EOF, matching POSIX behaviour.
            if err == ws::WSAESHUTDOWN {
                Ok(0)
            } else {
                Err(io::Error::from_raw_os_error(err))
            }
        } else {
            Ok(res as usize)
        }
    }

    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        // `IoSliceMut` is guaranteed ABI-compatible with `WSABUF` on
        // Windows, so we can pass the slice straight to `WSARecv`.
        let mut bytes_recvd: u32 = 0;
        let mut flags: u32 = 0;
        let res = unsafe {
            ws::WSARecv(
                self.inner.as_raw_socket() as _,
                bufs.as_mut_ptr() as *const WSABUF,
                bufs.len() as u32,
                &mut bytes_recvd,
                &mut flags,
                ptr::null_mut(),
                None,
            )
        };
        if res == SOCKET_ERROR {
            let err = unsafe { ws::WSAGetLastError() };
            // Map shutdown to EOF, matching POSIX behaviour.
            if err == ws::WSAESHUTDOWN {
                Ok(0)
            } else {
                Err(io::Error::from_raw_os_error(err))
            }
        } else {
            Ok(bytes_recvd as usize)
        }
    }
}

impl io::Write for Socket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self).write(buf)
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        (&*self).write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Write for &Socket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let res = unsafe {
            ws::send(
                self.inner.as_raw_socket() as _,
                buf.as_ptr(),
                buf.len() as i32,
                0,
            )
        };
        if res == SOCKET_ERROR {
            Err(last_error())
        } else {
            Ok(res as usize)
        }
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        // `IoSlice` is guaranteed ABI-compatible with `WSABUF` on Windows.
        let mut bytes_sent: u32 = 0;
        let res = unsafe {
            ws::WSASend(
                self.inner.as_raw_socket() as _,
                bufs.as_ptr() as *const WSABUF,
                bufs.len() as u32,
                &mut bytes_sent,
                0,
                ptr::null_mut(),
                None,
            )
        };
        if res == SOCKET_ERROR {
            Err(last_error())
        } else {
            Ok(bytes_sent as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsRawSocket for Socket {
    fn as_raw_socket(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

impl IntoRawSocket for Socket {
    fn into_raw_socket(self) -> RawSocket {
        self.inner.into_raw_socket()
    }
}

impl FromRawSocket for Socket {
    unsafe fn from_raw_socket(sock: RawSocket) -> Self {
        Socket {
            inner: unsafe { OwnedSocket::from_raw_socket(sock) },
        }
    }
}

impl From<Socket> for OwnedSocket {
    fn from(socket: Socket) -> OwnedSocket {
        socket.inner
    }
}

impl From<OwnedSocket> for Socket {
    fn from(inner: OwnedSocket) -> Socket {
        Socket { inner }
    }
}

impl fmt::Debug for Socket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Socket")
            .field("socket", &self.inner.as_raw_socket())
            .finish()
    }
}

const SUN_PATH_OFFSET: usize = mem::offset_of!(SOCKADDR_UN, sun_path);

/// Create a new non-blocking `AF_UNIX, SOCK_STREAM` socket.
pub(super) fn new_socket() -> io::Result<OwnedSocket> {
    let raw = net::new_socket(AF_UNIX as u32, SOCK_STREAM)?;
    // SAFETY: `net::new_socket` returned a valid SOCKET on success.
    Ok(unsafe { OwnedSocket::from_raw_socket(raw as RawSocket) })
}

fn sockaddr_un(path: &Path) -> io::Result<(SOCKADDR_UN, i32)> {
    let mut addr: SOCKADDR_UN = unsafe { mem::zeroed() };
    addr.sun_family = AF_UNIX;

    let bytes = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path must be valid UTF-8"))?
        .as_bytes();

    if bytes.len() >= addr.sun_path.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "path too long"));
    }

    unsafe {
        ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            addr.sun_path.as_mut_ptr().cast(),
            bytes.len(),
        );
    }

    let len = (SUN_PATH_OFFSET + bytes.len() + 1) as i32;
    Ok((addr, len))
}

pub(super) fn last_error() -> io::Error {
    io::Error::from_raw_os_error(unsafe { ws::WSAGetLastError() })
}

pub(super) fn cvt(result: i32) -> io::Result<i32> {
    if result == SOCKET_ERROR {
        Err(last_error())
    } else {
        Ok(result)
    }
}
