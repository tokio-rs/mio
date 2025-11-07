use super::{startup, wsa_error};
use std::{ffi::CStr, fmt::Debug, io, net::Shutdown, os::raw::c_int, path::Path, ptr::null_mut};
use windows_sys::Win32::Networking::WinSock::{
    self, AF_UNIX, FIONBIO, INVALID_SOCKET, SOCKADDR, SOCKADDR_UN, SOCKET, SOCKET_ERROR,
    SOCK_STREAM, SOL_SOCKET, SO_ERROR, WSABUF,
};
#[derive(Debug)]
pub(crate) struct Socket(pub SOCKET);

impl Socket {
    pub fn new() -> io::Result<Self> {
        unsafe {
            startup()?;
            match WinSock::socket(AF_UNIX as _, SOCK_STREAM, 0) {
                INVALID_SOCKET => Err(wsa_error()),
                s => Ok(Self(s)),
            }
        }
    }
    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        unsafe {
            match WinSock::send(self.0 as _, buf.as_ptr(), buf.len() as _, 0) {
                SOCKET_ERROR => Err(wsa_error()),
                len => Ok(len as _),
            }
        }
    }
    pub fn write_vectored(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        let bufs: Vec<_> = bufs
            .iter()
            .map(|buf| WSABUF {
                buf: buf.as_ptr() as *mut _,
                len: buf.len() as _,
            })
            .collect();
        let mut bytes_send = 0;
        unsafe {
            match WinSock::WSASend(
                self.0,
                bufs.as_ptr(),
                bufs.len() as _,
                &mut bytes_send,
                0,
                null_mut(),
                None,
            ) {
                0 => Ok(bytes_send as usize),
                _ => Err(wsa_error()),
            }
        }
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        unsafe {
            match WinSock::recv(self.0 as _, buf.as_mut_ptr(), buf.len() as _, 0) {
                0 => Err(io::Error::other("Connection closed")),
                len if len > 0 => Ok(len as _),
                _ => Err(wsa_error()),
            }
        }
    }
    pub fn recv_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        unsafe {
            let mut bytes_received = 0;
            let mut flags = 0;
            let mut bufs: Vec<_> = bufs
                .iter_mut()
                .map(|buf| WSABUF {
                    len: buf.len() as _,
                    buf: buf.as_mut_ptr(),
                })
                .collect();
            match WinSock::WSARecv(
                self.0,
                bufs.as_mut_ptr(),
                bufs.len() as _,
                &mut bytes_received,
                &mut flags,
                null_mut(),
                None,
            ) {
                0 => Ok(bytes_received as usize),
                _ => Err(wsa_error()),
            }
        }
    }
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        use WinSock::{SD_BOTH, SD_RECEIVE, SD_SEND};
        let shutdown_how = match how {
            Shutdown::Read => SD_RECEIVE,
            Shutdown::Write => SD_SEND,
            Shutdown::Both => SD_BOTH,
        };
        unsafe {
            match WinSock::shutdown(self.0, shutdown_how) {
                0 => Ok(()),
                _ => Err(wsa_error()),
            }
        }
    }
    pub fn accept(&self, addr: *mut SOCKADDR, addrlen: *mut i32) -> io::Result<Socket> {
        unsafe {
            // or we should just use None None here because
            // seems like accept write nothing to addr and addrlen
            match WinSock::accept(self.0, addr, addrlen) {
                INVALID_SOCKET => Err(wsa_error()),
                s => Ok(Socket(s)),
            }
        }
    }
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        let mut addr = SocketAddr::default();
        addr.addrlen = size_of::<SOCKADDR_UN>() as i32;
        match unsafe {
            WinSock::getsockname(
                self.0,
                &mut addr.addr as *mut _ as *mut _,
                &mut addr.addrlen as *mut _ as *mut _,
            )
        } {
            SOCKET_ERROR => Err(wsa_error()),
            _ => Ok(addr),
        }
    }
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        let mut addr = SocketAddr::default();
        addr.addrlen = size_of::<SOCKADDR_UN>() as i32;
        match unsafe {
            WinSock::getpeername(
                self.0,
                &mut addr.addr as *mut _ as *mut _,
                &mut addr.addrlen as *mut _ as *mut _,
            )
        } {
            SOCKET_ERROR => Err(wsa_error()),
            _ => Ok(addr),
        }
    }
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        unsafe {
            let mut val = c_int::default();
            let mut len = size_of::<c_int>() as i32;
            match WinSock::getsockopt(
                self.0,
                SOL_SOCKET,
                SO_ERROR,
                &mut val as *mut _ as *mut _,
                &mut len as *mut _,
            ) {
                SOCKET_ERROR => Err(wsa_error()),
                _ => {
                    if val == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(wsa_error()))
                    }
                }
            }
        }
    }
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        let mut val = if nonblocking { 1u32 } else { 0 };
        match unsafe { WinSock::ioctlsocket(self.0, FIONBIO, &mut val as *mut _) } {
            SOCKET_ERROR => Err(wsa_error()),
            _ => Ok(()),
        }
    }
}
#[derive(Default)]
/// A socket address for Unix domain sockets.
///
/// This struct wraps the underlying system socket address structure
/// along with its length to provide a safe interface for working with
/// Unix domain sockets.
pub struct SocketAddr {
    /// The underlying system socket address structure
    pub addr: SOCKADDR_UN,
    /// The length of the socket address structure
    pub addrlen: i32,
}

impl Debug for SocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> core::fmt::Result {
        let sun_path_str = unsafe { CStr::from_ptr(self.addr.sun_path.as_ptr()).to_string_lossy() };

        write!(
            f,
            "SocketAddr {{ addr: SOCKADDR_UN {{ sun_family: {}, sun_path: {:?} }}, addrlen: {} }}",
            self.addr.sun_family, sun_path_str, self.addrlen
        )
    }
}
impl SocketAddr {
    /// Creates a new `SocketAddr` from a filesystem path.
    ///
    /// # Arguments
    ///
    /// * `path` - A path to a socket file in the filesystem
    ///
    /// # Returns
    ///
    /// Returns `Ok(SocketAddr)` if the address was successfully created,
    /// or an `io::Error` if the path is invalid or too long.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use mio::uds::SocketAddr;
    ///
    /// let addr = SocketAddr::from_pathname("/tmp/socket.sock").unwrap();
    /// ```
    pub fn from_pathname<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let (addr, addrlen) = socketaddr_un(path.as_ref())?;
        Ok(Self { addr, addrlen })
    }
    /// Returns the contents of this address if it is a `pathname` address
    pub fn as_pathname(&self) -> Option<&Path> {
        let path_ptr = self.addr.sun_path.as_ptr();
        if unsafe { *path_ptr } == 0 {
            return None;
        }
        let c_str = unsafe { CStr::from_ptr(path_ptr) };
        match c_str.to_str() {
            Ok(s) => Some(Path::new(s)),
            Err(_e) => None,
        }
    }
}

pub(crate) fn socketaddr_un(path: &Path) -> io::Result<(SOCKADDR_UN, i32)> {
    // let bytes = path.as_os_str().as_encoded_bytes();
    let mut sockaddr = SOCKADDR_UN::default();
    // Winsock2 expects 'sun_path' to be a Win32 UTF-8 file system path
    let bytes = path.to_str().map(|s| s.as_bytes()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains invalid characters",
        )
    })?;

    if bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "paths may not contain interior null bytes",
        ));
    }

    if bytes.len() >= sockaddr.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be shorter than SUN_LEN",
        ));
    }
    let src_i8 = unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const i8, bytes.len()) };
    sockaddr.sun_family = AF_UNIX;
    sockaddr.sun_path[..src_i8.len()].copy_from_slice(src_i8);
    let socklen = size_of::<SOCKADDR_UN>() as _;
    Ok((sockaddr, socklen))
}
