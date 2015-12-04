use {EventSet, Selector, PollOpt, Token};
use bytes::{Buf, MutBuf};

// Re-export the io::Result / Error types for convenience
pub use std::io::{Read, Write, Result, Error, ErrorKind};

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
            unsafe { buf.advance(cnt); }
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
        self.read(dst).map_non_block()
    }
}

impl<T: Write> TryWrite for T {
    fn try_write(&mut self, src: &[u8]) -> Result<Option<usize>> {
        self.write(src).map_non_block()
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

/// A helper trait to provide the map_non_block function on Results.
pub trait MapNonBlock<T> {
    /// Maps a `Result<T>` to a `Result<Option<T>>` by converting
    /// operation-would-block errors into `Ok(None)`.
    fn map_non_block(self) -> Result<Option<T>>;
}

impl<T> MapNonBlock<T> for Result<T> {
    fn map_non_block(self) -> Result<Option<T>> {
        use std::io::ErrorKind::WouldBlock;

        match self {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                if let WouldBlock = err.kind() {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }
}
