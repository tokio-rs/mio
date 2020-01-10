use std::io::Result;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

#[derive(Debug)]
pub(crate) struct Socket {}

impl Socket {
    pub(crate) fn new(_: libc::c_int, _: libc::c_int, _: libc::c_int) -> Result<Self> {
        os_required!()
    }
}

#[cfg(unix)]
impl AsRawFd for Socket {
    fn as_raw_fd(&self) -> RawFd {
        os_required!()
    }
}

#[cfg(unix)]
impl FromRawFd for Socket {
    unsafe fn from_raw_fd(_: RawFd) -> Self {
        os_required!()
    }
}

#[cfg(unix)]
impl IntoRawFd for Socket {
    fn into_raw_fd(self) -> RawFd {
        os_required!()
    }
}
