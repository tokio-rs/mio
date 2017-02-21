use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::io::{IntoRawFd, AsRawFd, FromRawFd, RawFd};

use libc;

use {io, Ready, Poll, PollOpt, Token};
use event::Evented;
use unix::EventedFd;
use sys::unix::cvt;

pub fn set_nonblock(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        let mut nonblocking = 1 as libc::c_ulong;
        cvt(libc::ioctl(fd, libc::FIONBIO, &mut nonblocking)).map(|_| ())
    }
}

pub fn set_cloexec(fd: libc::c_int) -> io::Result<()> {
    unsafe {
        cvt(libc::ioctl(fd, libc::FIOCLEX)).map(|_| ())
    }
}

/*
 *
 * ===== Basic IO type =====
 *
 */

#[derive(Debug)]
pub struct Io {
    fd: File,
}

impl Io {
    pub fn try_clone(&self) -> io::Result<Io> {
        Ok(Io { fd: try!(self.fd.try_clone()) })
    }
}

impl FromRawFd for Io {
    unsafe fn from_raw_fd(fd: RawFd) -> Io {
        Io { fd: File::from_raw_fd(fd) }
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
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
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
