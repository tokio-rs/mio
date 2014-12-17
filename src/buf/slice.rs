use std::cmp;
use std::io::IoResult;
use super::{Buf, MutBuf};

pub struct SliceBuf<'a> {
    bytes: &'a [u8],
    pos: uint
}

impl<'a> SliceBuf<'a> {
    pub fn wrap(bytes: &'a [u8]) -> SliceBuf<'a> {
        SliceBuf { bytes: bytes, pos: 0 }
    }
}

impl<'a> Buf for SliceBuf<'a> {
    fn remaining(&self) -> uint {
        self.bytes.len() - self.pos
    }

    fn bytes<'b>(&'b self) -> &'b [u8] {
        self.bytes.slice_from(self.pos)
    }

    fn advance(&mut self, mut cnt: uint) {
        cnt = cmp::min(cnt, self.remaining());
        self.pos += cnt;
    }
}

impl<'a> Reader for SliceBuf<'a> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        super::read(self, buf)
    }
}

pub struct MutSliceBuf<'a> {
    bytes: &'a mut [u8],
    pos: uint
}

impl<'a> MutSliceBuf<'a> {
    pub fn wrap(bytes: &'a mut [u8]) -> MutSliceBuf<'a> {
        MutSliceBuf {
            bytes: bytes,
            pos: 0
        }
    }
}

impl<'a> Buf for MutSliceBuf<'a> {
    fn remaining(&self) -> uint {
        self.bytes.len() - self.pos
    }

    fn bytes<'b>(&'b self) -> &'b [u8] {
        self.bytes.slice_from(self.pos)
    }

    fn advance(&mut self, mut cnt: uint) {
        cnt = cmp::min(cnt, self.remaining());
        self.pos += cnt;
    }
}

impl<'a> MutBuf for MutSliceBuf<'a> {
    fn mut_bytes<'b>(&'b mut self) -> &'b mut [u8] {
        self.bytes.slice_from_mut(self.pos)
    }
}

impl<'a> Reader for MutSliceBuf<'a> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        super::read(self, buf)
    }
}

impl<'a> Writer for MutSliceBuf<'a> {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        super::write(self, buf)
    }
}
