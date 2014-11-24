use buf::{Buf, MutBuf};
use os;
use error::MioResult;
use self::NonBlock::{Ready, WouldBlock};
use error::MioErrorKind as mek;

#[deriving(Show)]
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
            Ready(v) => v,
            _ => panic!("would have blocked, no result to take")
        }
    }
}

pub trait IoHandle {
    fn desc(&self) -> &os::IoDesc;
}

pub trait IoReader {
    fn read(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<(uint)>>;
}

pub trait IoWriter {
    fn write(&mut self, buf: &mut Buf) -> MioResult<NonBlock<(uint)>>;
}

pub trait IoAcceptor<T> {
    fn accept(&mut self) -> MioResult<NonBlock<T>>;
}

pub fn pipe() -> MioResult<(PipeReader, PipeWriter)> {
    let (rd, wr) = try!(os::pipe());
    Ok((PipeReader { desc: rd }, PipeWriter { desc: wr }))
}

pub struct PipeReader {
    desc: os::IoDesc
}

impl IoHandle for PipeReader {
    fn desc(&self) -> &os::IoDesc {
        &self.desc
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

impl IoReader for PipeReader {
    fn read(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<(uint)>> {
        read(self, buf)
    }
}

impl IoWriter for PipeWriter {
    fn write(&mut self, buf: &mut Buf) -> MioResult<NonBlock<(uint)>> {
        write(self, buf)
    }
}

/// Reads the length of the slice supplied by buf.mut_bytes into the buffer
/// This is not guaranteed to consume an entire datagram or segment.
/// If your protocol is msg based (instead of continuous stream) you should
/// ensure that your buffer is large enough to hold an entire segment (1532 bytes if not jumbo
/// frames)
#[inline]
pub fn read<I: IoHandle>(io: &mut I, buf: &mut MutBuf) -> MioResult<NonBlock<uint>> {

    match os::read(io.desc(), buf.mut_bytes()) {
        // Successfully read some bytes, advance the cursor
        Ok(cnt) => {
            buf.advance(cnt);
            Ok(Ready(cnt))
        }
        Err(e) => {
            match e.kind {
                mek::WouldBlock => Ok(WouldBlock),
                _ => Err(e)
            }
        }
    }
}

///writes the length of the slice supplied by Buf.bytes into the socket
#[inline]
pub fn write<O: IoHandle>(io: &mut O, buf: &mut Buf) -> MioResult<NonBlock<uint>> {
    match os::write(io.desc(), buf.bytes()) {
        Ok(cnt) => { buf.advance(cnt); Ok(Ready(cnt)) }
        Err(e) => {
            match e.kind {
                mek::WouldBlock => Ok(WouldBlock),
                _               => Err(e)
            }
        }
    }
}
