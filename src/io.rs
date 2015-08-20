use {EventSet, PollOpt, Token};
use sys::Selector;
use bytes::{Buf, MutBuf};

// Re-export the io::Result / Error types for convenience
pub use std::io::{Read, Write, Result, Error};

/// A value that may be registered with an `EventLoop`
pub trait Evented {
    #[doc(hidden)]
    fn register(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> Result<()>;

    #[doc(hidden)]
    fn reregister(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> Result<()>;

    #[doc(hidden)]
    fn deregister(&self, selector: &mut Selector) -> Result<()>;
}

pub trait TryRead {
    fn try_read_buf<B: MutBuf>(&mut self, buf: &mut B) -> Result<Option<usize>>
        where Self : Sized
    {
        // Reads the length of the slice supplied by buf.mut_bytes into the buffer
        // This is not guaranteed to consume an entire datagram or segment.
        // If your protocol is msg based (instead of continuous stream) you should
        // ensure that your buffer is large enough to hold an entire segment (1532 bytes if not jumbo
        // frames)
        let res = self.try_read(unsafe { buf.mut_bytes() });

        if let Ok(Some(cnt)) = res {
            buf.advance(cnt);
        }

        res
    }

    fn try_read(&mut self, buf: &mut [u8]) -> Result<Option<usize>>;
}

pub trait TryWrite {
    fn try_write_buf<B: Buf>(&mut self, buf: &mut B) -> Result<Option<usize>>
        where Self : Sized
    {
        let res = self.try_write(buf.bytes());

        if let Ok(Some(cnt)) = res {
            buf.advance(cnt);
        }

        res
    }

    fn try_write(&mut self, buf: &[u8]) -> Result<Option<usize>>;
}

impl<T: Read> TryRead for T {
    fn try_read(&mut self, dst: &mut [u8]) -> Result<Option<usize>> {
        self.read(dst)
            .map(|cnt| Some(cnt))
            .or_else(to_non_block)
    }
}

impl<T: Write> TryWrite for T {
    fn try_write(&mut self, src: &[u8]) -> Result<Option<usize>> {
        self.write(src)
            .map(|cnt| Some(cnt))
            .or_else(to_non_block)
    }
}

pub trait TryAccept {
    type Output;

    fn accept(&self) -> Result<Option<Self::Output>>;
}

/*
 *
 * ===== Helpers =====
 *
 */

pub fn to_non_block<T>(err: Error) -> Result<Option<T>> {
    use std::io::ErrorKind::WouldBlock;

    if let WouldBlock = err.kind() {
        return Ok(None);
    }

    Err(err)
}
