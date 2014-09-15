use buf::{Buf, MutBuf};
use os;
use error::MioResult;

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
            _ => fail!("would have blocked, no result to take")
        }
    }
}

pub trait IoHandle {
    fn desc(&self) -> &os::IoDesc;
}

pub trait IoReader {
    fn read(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<()>>;
}

pub trait IoWriter {
    fn write(&mut self, buf: &mut Buf) -> MioResult<NonBlock<()>>;
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
    fn read(&mut self, buf: &mut MutBuf) -> MioResult<NonBlock<()>> {
        read(self, buf)
    }
}

impl IoWriter for PipeWriter {
    fn write(&mut self, buf: &mut Buf) -> MioResult<NonBlock<()>> {
        write(self, buf)
    }
}

pub fn read<I: IoHandle>(io: &mut I, buf: &mut MutBuf) -> MioResult<NonBlock<()>> {
    let mut first_iter = true;

    while buf.has_remaining() {
        match os::read(io.desc(), buf.mut_bytes()) {
            // Successfully read some bytes, advance the cursor
            Ok(cnt) => {
                buf.advance(cnt);
                first_iter = false;
            }
            Err(e) => {
                if e.is_would_block() {
                    return Ok(WouldBlock);
                }

                // If the EOF is hit the first time around, then propagate
                if e.is_eof() {
                    if first_iter {
                        return Err(e);
                    }

                    return Ok(Ready(()));
                }

                // Indicate that the read was successful
                return Err(e);
            }
        }
    }

    Ok(Ready(()))
}

pub fn write<O: IoHandle>(io: &mut O, buf: &mut Buf) -> MioResult<NonBlock<()>> {
    while buf.has_remaining() {
        match os::write(io.desc(), buf.bytes()) {
            Ok(cnt) => buf.advance(cnt),
            Err(e) => {
                if e.is_would_block() {
                    return Ok(WouldBlock);
                }

                return Err(e);
            }
        }
    }

    Ok(Ready(()))
}
