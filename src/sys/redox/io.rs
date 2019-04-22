use std::fs::File;
use std::io::{Read, Write};
use std::mem;
use std::os::unix::io::{IntoRawFd, AsRawFd, FromRawFd, RawFd};
use syscall::{fcntl, F_SETFL, F_GETFL, O_NONBLOCK};
use {io, poll, Evented, Ready, Poll, PollOpt, Token};

pub fn set_nonblock(fd: RawFd) -> io::Result<()> {
    let flags = fcntl(fd as usize, F_GETFL, 0).map_err(super::from_syscall_error)?;
    fcntl(fd as usize, F_SETFL, flags | O_NONBLOCK).map_err(super::from_syscall_error)
                                                     .map(|_| ())
}

/*
 *
 * ===== Basic IO type =====
 *
 */

/// Manages a FD
#[derive(Debug)]
pub struct Io {
    file: File,
}

impl FromRawFd for Io {
    unsafe fn from_raw_fd(fd: RawFd) -> Io {
        Io { file: File::from_raw_fd(fd) }
    }
}

impl IntoRawFd for Io {
    fn into_raw_fd(self) -> RawFd {
        // Forget self to prevent drop() from closing self.fd.
        let fd = self.as_raw_fd();
        mem::forget(self);
        fd
    }
}

impl AsRawFd for Io {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

impl Evented for Io {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        poll::selector(poll).register(self.as_raw_fd(), token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        poll::selector(poll).reregister(self.as_raw_fd(), token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        poll::selector(poll).deregister(self.as_raw_fd())
    }
}

impl Read for Io {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        self.file.read(dst)
    }
}

impl<'a> Read for &'a Io {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        (&self.file).read(dst)
    }
}

impl Write for Io {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        self.file.write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> Write for &'a Io {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        (&self.file).write(src)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
