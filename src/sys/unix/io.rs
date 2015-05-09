use {io, Evented, Interest, PollOpt, Selector, Token, TryRead, TryWrite};
use unix::FromRawFd;
use std::os::unix::io::{AsRawFd, RawFd};

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
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        selector.register(self.fd, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        selector.reregister(self.fd, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        selector.deregister(self.fd)
    }
}

impl TryRead for Io {
    fn read_slice(&mut self, dst: &mut [u8]) -> io::Result<Option<usize>> {
        use nix::unistd::read;

        read(self.as_raw_fd(), dst)
            .map(|cnt| Some(cnt))
            .map_err(super::from_nix_error)
            .or_else(io::to_non_block)
    }
}

impl TryWrite for Io {
    fn write_slice(&mut self, src: &[u8]) -> io::Result<Option<usize>> {
        use nix::unistd::write;

        write(self.as_raw_fd(), src)
            .map_err(super::from_nix_error)
            .map(|cnt| Some(cnt))
            .or_else(io::to_non_block)
    }
}

impl Drop for Io {
    fn drop(&mut self) {
        use nix::unistd::close;
        let _ = close(self.as_raw_fd());
    }
}
