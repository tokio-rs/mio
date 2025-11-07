use super::{startup, wsa_error};
use std::{ffi::CStr, fmt::Debug, io, net::Shutdown, os::raw::c_int, path::Path, time::Duration};
use windows_sys::Win32::Networking::WinSock::{
    self, AF_UNIX, FIONBIO, INVALID_SOCKET, SOCKADDR, SOCKADDR_UN, SOCKET, SOCKET_ERROR,
    SOCK_STREAM, SOL_SOCKET, SO_ERROR,
};
#[derive(Debug)]
pub struct Socket(pub SOCKET);

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
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        unsafe {
            match WinSock::recv(self.0 as _, buf.as_mut_ptr(), buf.len() as _, 0) {
                0 => Err(io::Error::other("Connection closed")),
                len if len > 0 => Ok(len as _),
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
        let mut s = SocketAddr::default();
        match unsafe {
            WinSock::getpeername(
                self.0,
                &mut s.addr as *mut _ as *mut _,
                &mut s.addrlen as *mut _ as *mut _,
            )
        } {
            SOCKET_ERROR => Err(wsa_error()),
            _ => Ok(s),
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
    pub fn set_timeout(&self, dur: Option<Duration>, kind: i32) -> io::Result<()> {
        let timeout = match dur {
            Some(dur) => dur.as_millis() as u32,
            None => 0,
        }
        .to_ne_bytes();
        match unsafe {
            WinSock::setsockopt(
                self.0,
                SOL_SOCKET,
                kind,
                &timeout as *const _,
                timeout.len() as _,
            )
        } {
            SOCKET_ERROR => Err(wsa_error()),
            _ => Ok(()),
        }
    }
    //seems like not support
    //https://learn.microsoft.com/en-us/windows/win32/api/winsock/nf-winsock-getsockopt
    pub fn timeout(&self, kind: i32) -> io::Result<Option<Duration>> {
        let mut val = c_int::default();
        let mut len = size_of::<c_int>();
        match unsafe {
            WinSock::getsockopt(
                self.0,
                SOL_SOCKET,
                kind,
                &mut val as *mut _ as *mut _,
                &mut len as *mut _ as *mut _,
            )
        } {
            SOCKET_ERROR => Err(wsa_error()),
            _ => Ok(Some(Duration::from_millis(val as u64))),
        }
    }
}
#[derive(Default)]
pub struct SocketAddr {
    pub addr: SOCKADDR_UN,
    pub addrlen: i32,
}

impl Debug for SocketAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> core::fmt::Result {
        let sun_path_str = unsafe {
            CStr::from_ptr(self.addr.sun_path.as_ptr())
                .to_string_lossy()
        };
        
        write!(f, "SocketAddr {{ addr: SOCKADDR_UN {{ sun_family: {}, sun_path: {:?} }}, addrlen: {} }}",
               self.addr.sun_family, sun_path_str, self.addrlen)
    }
}
impl SocketAddr {
    pub fn from_pathname<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let (addr, addrlen) = socketaddr_un(path.as_ref())?;
        Ok(Self { addr, addrlen })
    }
}

pub fn socketaddr_un(path: &Path) -> io::Result<(SOCKADDR_UN, i32)> {
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
