use buf::{Buf, MutBuf};
use std::os::unix::{Fd, AsRawFd};

// Re-export the io::Result / Error types for convenience
pub use std::io::{Result, Error};

/// A value that may be registered with an `EventLoop`
pub trait Evented : AsRawFd {
}

/// Create a value with a FD
pub trait FromFd {
    fn from_fd(fd: Fd) -> Self;
}

pub trait TryRead {
    fn read<B: MutBuf>(&mut self, buf: &mut B) -> Result<Option<usize>> {
        // Reads the length of the slice supplied by buf.mut_bytes into the buffer
        // This is not guaranteed to consume an entire datagram or segment.
        // If your protocol is msg based (instead of continuous stream) you should
        // ensure that your buffer is large enough to hold an entire segment (1532 bytes if not jumbo
        // frames)
        let res = self.read_slice(buf.mut_bytes());

        if let Ok(Some(cnt)) = res {
            buf.advance(cnt);
        }

        res
    }

    fn read_slice(&mut self, buf: &mut [u8]) -> Result<Option<usize>>;
}

pub trait TryWrite {
    fn write<B: Buf>(&mut self, buf: &mut B) -> Result<Option<usize>> {
        let res = self.write_slice(buf.bytes());

        if let Ok(Some(cnt)) = res {
            buf.advance(cnt);
        }

        res
    }

    fn write_slice(&mut self, buf: &[u8]) -> Result<Option<usize>>;
}

/*
 *
 * ===== Basic IO type =====
 *
 */

#[derive(Debug)]
pub struct Io {
    fd: Fd,
}

impl Io {
    pub fn new(fd: Fd) -> Io {
        Io { fd: fd }
    }
}

impl AsRawFd for Io {
    fn as_raw_fd(&self) -> Fd {
        self.fd
    }
}

impl Evented for Io {
}

impl TryRead for Io {
    fn read_slice(&mut self, dst: &mut [u8]) -> Result<Option<usize>> {
        use nix::unistd::read;

        read(self.as_raw_fd(), dst)
            .map(|cnt| Some(cnt))
            .map_err(from_nix_error)
            .or_else(to_non_block)
    }
}

impl TryWrite for Io {
    fn write_slice(&mut self, src: &[u8]) -> Result<Option<usize>> {
        use nix::unistd::write;

        write(self.as_raw_fd(), src)
            .map_err(from_nix_error)
            .map(|cnt| Some(cnt))
            .or_else(to_non_block)
    }
}

impl Drop for Io {
    fn drop(&mut self) {
        use nix::unistd::close;
        let _ = close(self.as_raw_fd());
    }
}

/*
 *
 * ===== Pipe =====
 *
 */

pub fn pipe() -> Result<(PipeReader, PipeWriter)> {
    use nix::fcntl::{O_NONBLOCK, O_CLOEXEC};
    use nix::unistd::pipe2;

    let (rd, wr) = try!(pipe2(O_NONBLOCK | O_CLOEXEC)
        .map_err(from_nix_error));

    let rd = FromFd::from_fd(rd);
    let wr = FromFd::from_fd(wr);

    Ok((rd, wr))
}

pub struct PipeReader {
    io: Io,
}

impl FromFd for PipeReader {
    fn from_fd(fd: Fd) -> PipeReader {
        PipeReader { io: Io::new(fd) }
    }
}

impl AsRawFd for PipeReader {
    fn as_raw_fd(&self) -> Fd {
        self.io.as_raw_fd()
    }
}

impl Evented for PipeReader {
}

impl TryRead for PipeReader {
    fn read_slice(&mut self, buf: &mut [u8]) -> Result<Option<usize>> {
        self.io.read_slice(buf)
    }
}
pub struct PipeWriter {
    io: Io,
}

impl FromFd for PipeWriter {
    fn from_fd(fd: Fd) -> PipeWriter {
        PipeWriter { io: Io::new(fd) }
    }
}

impl AsRawFd for PipeWriter {
    fn as_raw_fd(&self) -> Fd {
        self.io.fd
    }
}

impl Evented for PipeWriter {
}

impl TryWrite for PipeWriter {
    fn write_slice(&mut self, buf: &[u8]) -> Result<Option<usize>> {
        self.io.write_slice(buf)
    }
}

/*
 *
 * ===== Helpers =====
 *
 */

pub fn from_nix_error(err: ::nix::NixError) -> Error {
    use nix::{errno, NixError};

    match err {
        NixError::Sys(errno) => Error::from_os_error(errno as i32),
        _ => Error::from_os_error(errno::EINVAL as i32)
    }
}

pub fn to_non_block<T>(err: Error) -> Result<Option<T>> {
    use std::io::ErrorKind::ResourceUnavailable;

    if let ResourceUnavailable = err.kind() {
        return Ok(None);
    }

    Err(err)
}
