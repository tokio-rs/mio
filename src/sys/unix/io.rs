use {io, Evented, EventSet, PollOpt, Selector, Token};
use std::io::{Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

/*
 *
 * ===== Basic IO type =====
 *
 */

#[derive(Debug)]
pub struct Io {
    fd: RawFd,
}

impl Io {
    pub fn from_raw_fd(fd: RawFd) -> Io {
        Io { fd: fd }
    }
}

impl From<RawFd> for Io {
    fn from(fd: RawFd) -> Io {
        Io { fd: fd }
    }
}

impl FromRawFd for Io {
    unsafe fn from_raw_fd(fd: RawFd) -> Io {
        From::from(fd)
    }
}

impl AsRawFd for Io {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Evented for Io {
    fn register(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        selector.register(self.fd, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        selector.reregister(self.fd, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        selector.deregister(self.fd)
    }
}

impl Read for Io {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        <&Io as Read>::read(&mut &*self, dst)
    }
}

impl<'a> Read for &'a Io {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        use nix::unistd::read;

        read(self.as_raw_fd(), dst)
            .map_err(super::from_nix_error)
    }
}

impl Write for Io {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        <&Io as Write>::write(&mut &*self, src)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> Write for &'a Io {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        use nix::unistd::write;

        write(self.as_raw_fd(), src)
            .map_err(super::from_nix_error)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Io {
    fn drop(&mut self) {
        use nix::unistd::close;
        let _ = close(self.as_raw_fd());
    }
}
