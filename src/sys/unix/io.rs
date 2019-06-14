use crate::event::Evented;
use crate::sys::unix::cvt;
use crate::unix::EventedFd;
use crate::{Interests, PollOpt, Registry, Token};
use libc;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

pub fn set_nonblock(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        cvt(libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK)).map(|_| ())
    }
}

pub fn set_cloexec(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        cvt(libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC)).map(|_| ())
    }
}

/*
 *
 * ===== Basic IO type =====
 *
 */

/// Manages a FD
#[derive(Debug)]
pub struct Io {
    fd: File,
}

impl Io {
    /// Try to clone the FD
    pub fn try_clone(&self) -> io::Result<Io> {
        Ok(Io {
            fd: self.fd.try_clone()?,
        })
    }
}

impl FromRawFd for Io {
    unsafe fn from_raw_fd(fd: RawFd) -> Io {
        Io {
            fd: File::from_raw_fd(fd),
        }
    }
}

impl IntoRawFd for Io {
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

impl AsRawFd for Io {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl Evented for Io {
    fn register(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).register(registry, token, interests, opts)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(registry, token, interests, opts)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(registry)
    }
}

impl Read for Io {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        (&self.fd).read(dst)
    }
}

impl<'a> Read for &'a Io {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        (&self.fd).read(dst)
    }
}

impl Write for Io {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        (&self.fd).write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.fd).flush()
    }
}

impl<'a> Write for &'a Io {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        (&self.fd).write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.fd).flush()
    }
}
