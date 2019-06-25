use std::io;

use crate::sys::unix::cvt;

pub fn set_nonblock(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        cvt(libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK)).map(|_| ())
    }
}

#[cfg(any(
    target_os = "bitrig",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "ios",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub fn set_cloexec(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        cvt(libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC)).map(|_| ())
    }
}
