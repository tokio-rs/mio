use {os, MioResult, MioError};
use buf::{Buf, MutBuf};

pub use os::IoDesc;

/// The result of a non-blocking operation.
#[derive(Debug)]
pub enum NonBlock<T> {
    Ready(T),
    WouldBlock
}

impl<T> NonBlock<T> {
    pub fn would_block(&self) -> bool {
        match *self {
            NonBlock::WouldBlock => true,
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

pub trait IoHandle {
    fn desc(&self) -> &IoDesc;
}

pub trait FromIoDesc {
    fn from_desc(desc: IoDesc) -> Self;
}

pub trait IoReader {
    fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>>;
    fn read_slice(&self, buf: &mut [u8]) -> MioResult<NonBlock<usize>>;
}

pub trait IoWriter {
    fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>>;
    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>>;
}

pub trait IoAcceptor {
    type Output;
    fn accept(&mut self) -> MioResult<NonBlock<Self::Output>>;
}

/*
 *
 * ===== Basic IO type =====
 *
 */

pub struct Io {
    desc: IoDesc,
}

impl Io {
    pub fn new(desc: IoDesc) -> Io {
        Io { desc: desc }
    }
}

impl IoHandle for Io {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
    }
}

impl IoReader for Io {
    fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>> {
        read(self, buf)
    }

    fn read_slice(&self, buf: &mut [u8]) -> MioResult<NonBlock<usize>> {
        read_slice(self, buf)
    }
}

impl IoWriter for Io {
    fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>> {
        write(self, buf)
    }

    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
        write_slice(self, buf)
    }
}

/*
 *
 * ===== Pipe =====
 *
 */

pub fn pipe() -> MioResult<(PipeReader, PipeWriter)> {
    use nix::fcntl::{O_NONBLOCK, O_CLOEXEC};
    use nix::unistd::pipe2;

    let (rd, wr) = try!(pipe2(O_NONBLOCK | O_CLOEXEC)
        .map_err(MioError::from_nix_error));

    let rd = PipeReader { desc: IoDesc { fd: rd }};
    let wr = PipeWriter { desc: IoDesc { fd: wr }};

    Ok((rd, wr))
}

pub struct PipeReader {
    desc: os::IoDesc
}

impl IoHandle for PipeReader {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
    }
}

impl FromIoDesc for PipeReader {
    fn from_desc(desc: IoDesc) -> Self {
        PipeReader { desc: desc }
    }
}

pub struct PipeWriter {
    desc: os::IoDesc
}

impl IoHandle for PipeWriter {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
    }
}

impl FromIoDesc for PipeWriter {
    fn from_desc(desc: IoDesc) -> Self {
        PipeWriter { desc: desc }
    }
}

impl IoReader for PipeReader {
    fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>> {
        read(self, buf)
    }

    fn read_slice(&self, buf: &mut [u8]) -> MioResult<NonBlock<usize>> {
        read_slice(self, buf)
    }
}

impl IoWriter for PipeWriter {
    fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>> {
        write(self, buf)
    }

    fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
        write_slice(self, buf)
    }
}

/*
 *
 * ===== Read / Write =====
 *
 */

/// Reads the length of the slice supplied by buf.mut_bytes into the buffer
/// This is not guaranteed to consume an entire datagram or segment.
/// If your protocol is msg based (instead of continuous stream) you should
/// ensure that your buffer is large enough to hold an entire segment (1532 bytes if not jumbo
/// frames)
#[inline]
pub fn read<I: IoHandle, B: MutBuf>(io: &I, buf: &mut B) -> MioResult<NonBlock<usize>> {
    let res = read_slice(io, buf.mut_bytes());

    if let Ok(NonBlock::Ready(cnt)) = res {
        buf.advance(cnt);
    }

    res
}

///writes the length of the slice supplied by Buf.bytes into the socket
///then advances the buffer that many bytes
#[inline]
pub fn write<O: IoHandle, B: Buf>(io: &O, buf: &mut B) -> MioResult<NonBlock<usize>> {
    let res = write_slice(io, buf.bytes());

    if let Ok(NonBlock::Ready(cnt)) = res {
        buf.advance(cnt);
    }

    res
}

///reads the length of the supplied slice from the socket into the slice
#[inline]
pub fn read_slice<I: IoHandle>(io: & I, dst: &mut [u8]) -> MioResult<NonBlock<usize>> {
    use nix::unistd::read;

    read(io.desc().fd, dst)
        .map_err(MioError::from_nix_error)
        .and_then(|cnt| {
            if cnt > 0 {
                Ok(NonBlock::Ready(cnt))
            } else {
                Err(MioError::eof())
            }
        })
        .or_else(to_non_block)
}

///writes the length of the supplied slice into the socket from the slice
#[inline]
pub fn write_slice<I: IoHandle>(io: & I, src: &[u8]) -> MioResult<NonBlock<usize>> {
    use nix::unistd::write;

    write(io.desc().fd, src)
        .map_err(MioError::from_nix_error)
        .map(|cnt| NonBlock::Ready(cnt))
        .or_else(to_non_block)
}

/*
 *
 * ===== Helpers =====
 *
 */

pub fn to_non_block<T>(err: MioError) -> MioResult<NonBlock<T>> {
    if err.is_would_block() {
        Ok(NonBlock::WouldBlock)
    } else {
        Err(err)
    }
}
