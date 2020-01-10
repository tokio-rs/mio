use crate::sys;

use std::io::Result;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

/// todo
#[derive(Debug)]
pub struct TcpSocket {
    inner: sys::Socket,
}

impl TcpSocket {
    /// todo
    pub fn new(domain: libc::c_int) -> Result<Self> {
        sys::Socket::new(domain, libc::SOCK_STREAM, 0).map(|socket| TcpSocket { inner: socket })
    }
}

#[cfg(unix)]
impl AsRawFd for TcpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

#[cfg(unix)]
impl FromRawFd for TcpSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        TcpSocket {
            inner: FromRawFd::from_raw_fd(fd),
        }
    }
}

#[cfg(unix)]
impl IntoRawFd for TcpSocket {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}
