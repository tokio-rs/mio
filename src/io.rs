use os::{self, FromFd};
use buf::{Buf, MutBuf};
use nix::NixError;
use std::io::{Read, Write, Result, Error, ErrorKind};
use std::os::unix::Fd;

pub trait Listenable {
    fn as_fd(&self) -> Fd;
}

/// The result of a non-blocking operation.
#[derive(Debug)]
pub enum NonBlock<T> {
    Ready(T),
    WouldBlock
}

impl<T> NonBlock<T> {
    pub fn would_block(&self) -> bool {
        match *self {
            WouldBlock => true,
            _ => false
        }
    }

    pub fn unwrap(self) -> T {
        match self {
            NonBlock::Ready(v) => v,
            _ => panic!("would have blocked, no result to take")
        }
    }
}

/// A trait for values which are byte-oriented non-blocking sources. Reads will
/// not block the calling thread. If the resource is ready, the read will
/// succeed. Otherwise, WouldBlock is returned.
pub trait TryRead : Read {

    /// Read into the given buffer
    fn read_buf<B: MutBuf>(&self, buf: &mut B) -> Result<NonBlock<usize>>;

    /// Read into the given byte slice
    fn read_slice(&self, buf: &mut [u8]) -> Result<NonBlock<usize>>;
}

impl<R: Read> TryRead for R {
    fn read_buf<B: MutBuf>(&self, buf: &mut B) -> Result<NonBlock<usize>> {
        let res = self.read_slice(buf.mut_bytes());

        if let Ok(NonBlock::Ready(cnt)) = res {
            buf.advance(cnt);
        }

        res
    }

    fn read_slice(&self, buf: &mut [u8]) -> Result<NonBlock<usize>> {
        self.read(buf)
            .map(|cnt| NonBlock::Ready(cnt))
            .or_else(to_non_block_res)
    }
}

/// A trait for values which are byte-oriented non-blocking sinks. Writes will
/// not block the calling thread. If the resources is ready, the write will
/// succeed. Otherwise, WouldBlock is returned.
pub trait TryWrite : Write {

    /// Write into the given buffer
    fn write_buf<B: Buf>(&self, buf: &mut B) -> Result<NonBlock<usize>>;

    /// Write into the given byte slice
    fn write_slice(&self, buf: &[u8]) -> Result<NonBlock<usize>>;

}

impl<W: Write> TryWrite for W {
    fn write_buf<B: Buf>(&self, buf: &mut B) -> Result<NonBlock<usize>> {
        let res = self.write_slice(buf.bytes());

        if let Ok(NonBlock::Ready(cnt)) = res {
            buf.advance(cnt);
        }

        res
    }

    fn write_slice(&self, buf: &[u8]) -> Result<NonBlock<usize>> {
        self.write(buf)
            .map(|cnt| Ok(NonBlock::Ready(cnt)))
            .or_else(to_non_block_res)
    }
}

/// Creates a new pipe, a pair of unidirectional channels that can be used for
/// in-process or inter-process communication.
///
/// [Further reading](http://man7.org/linux/man-pages/man7/pipe.7.html)
pub fn pipe() -> Result<(PipeWriter, PipeReader)> {
    let (rd, wr) = try!(os::pipe());
    Ok((FromFd::from_fd(wr), FromFd::from_fd(rd)))
}

/*
 *
 * ===== PipeReader =====
 *
 */

/// The read half of a pipe
pub struct PipeReader {
    fd: Fd,
}

impl PipeReader {
}

impl FromFd for PipeReader {
    fn from_fd(fd: Fd) -> Self {
        PipeReader { fd: fd }
    }
}

impl Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        unimplemented!();
    }
}

impl Drop for PipeReader {
    fn drop(&mut self) {
        os::close(self.fd)
    }
}

/*
 *
 * ===== PipeWriter =====
 *
 */

/// The write half of a pipe
pub struct PipeWriter {
    fd: Fd,
}

impl PipeWriter {
}

impl FromFd for PipeWriter {
    fn from_fd(fd: Fd) -> Self {
        PipeWriter { fd: fd }
    }
}

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        os::write(self.fd, buf)
    }
}

impl Drop for PipeWriter {
    fn drop(&mut self) {
        os::close(self.fd)
    }
}

/*
 *
 * ===== Errors =====
 *
 */

fn to_io_error(err: NixError) -> Error {
    unimplemented!();
}

fn to_non_block_res<T>(err: Error) -> Result<T> {
    match err.kind() {
        ErrorKind::ResourceUnavailable => Ok(NonBlock::WouldBlock),
        _ => Err(err),
    }
}
