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

    fn bytes<'a>(&'a self) -> &'a [u8] {
        self.bytes.slice_from(self.pos)
    }

    fn advance(&mut self, cnt: uint) {
        assert!(cnt <= self.remaining(), "advancing too far");
        self.pos += cnt;
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

    fn bytes<'a>(&'a self) -> &'a [u8] {
        self.bytes.slice_from(self.pos)
    }

    fn advance(&mut self, cnt: uint) {
        assert!(cnt <= self.remaining(), "advancing too far");
        self.pos += cnt;
    }
}

impl<'a> MutBuf for MutSliceBuf<'a> {
    fn mut_bytes<'a>(&'a mut self) -> &'a mut [u8] {
        self.bytes.mut_slice_from(self.pos)
    }
}
