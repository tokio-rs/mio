//! Various strategies for non-blocking sequential byte access
//!
use std::slice::bytes;
use std::{cmp, io};

pub use self::byte::ByteBuf;
pub use self::ring::{RingBuf, RingBufReader, RingBufWriter};
pub use self::slice::{SliceBuf, MutSliceBuf};

mod byte;
mod ring;
mod slice;

/* TODO
 * - Cursor that can take a slice and provide a temp buf
 */

pub trait Buf {
    fn remaining(&self) -> usize;
    fn bytes<'a>(&'a self) -> &'a [u8];
    fn advance(&mut self, cnt: usize);

    fn has_remaining(&self) -> bool {
        self.remaining() > 0
    }
}

pub trait MutBuf : Buf {
    fn mut_bytes<'a>(&'a mut self) -> &'a mut [u8];
}

pub fn wrap<'a>(bytes: &'a [u8]) -> SliceBuf<'a> {
    SliceBuf::wrap(bytes)
}

pub fn wrap_mut<'a>(bytes: &'a mut [u8]) -> MutSliceBuf<'a> {
    MutSliceBuf::wrap(bytes)
}

// TODO:
// It would be really nice to automatically implement Reader / Writer for all
// implementations of Buf, however this is not currently possible at the time
// that I am writing this. nmatsakis says that it should be implemented in a
// few weeks, so I will wait for this to land.
//
// Also, the default implementation for Reader / Writer fns is currently
// broken. If there are IO errors during execution, any data that has been read
// up to that point is lost.
//
// impl<B: Buf> Reader for B {
//
// }
//
// impl<B: MutBuf> Writer for B {
//
// }
//
// Instead, we provide these two fns we call in all the concrete implementations

fn read<B: Buf>(buf: &mut B, dst: &mut [u8]) -> io::IoResult<usize> {
    let nread = cmp::min(buf.remaining(), dst.len());

    if nread == 0 {
        if dst.len() == 0 {
            return Ok(0);
        }

        return Err(io::standard_error(io::EndOfFile));
    }

    let mut curr = 0u;

    while curr < nread {
        let cnt = {
            let src = buf.bytes();
            let cnt = cmp::min(src.len(), dst.len() - curr);

            // copy the bytes
            bytes::copy_memory(dst.slice_from_mut(curr), src.slice_to(cnt));

            // advance cursors
            cnt
        };

        curr += cnt;
        buf.advance(cnt);
    }

    Ok(nread)
}

fn write<B: MutBuf>(buf: &mut B, src: &[u8]) -> io::IoResult<()> {
    debug!("buf::write; buf={}; src={}", buf.remaining(), src.len());

    if src.len() > buf.remaining() {
        return Err(io::standard_error(io::EndOfFile));
    }

    let mut curr = 0u;

    while curr < src.len() {
        let cnt = {
            let dst = buf.mut_bytes();
            let cnt = cmp::min(dst.len(), src.len() - curr);

            bytes::copy_memory(dst, src.slice(curr, cnt));
            cnt
        };

        curr += cnt;
        buf.advance(cnt);
    }

    Ok(())
}
