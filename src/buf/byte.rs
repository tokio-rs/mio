use std::{cmp, mem, num, ptr};
use std::io::IoResult;
use std::raw;
use alloc::heap;
use super::{Buf, MutBuf};

pub struct ByteBuf {
    ptr: *mut u8,
    cap: uint,
    pos: uint,
    lim: uint
}

impl ByteBuf {
    pub fn new(mut capacity: uint) -> ByteBuf {
        // Handle 0 capacity case
        if capacity == 0 {
            return ByteBuf {
                ptr: ptr::mut_null(),
                cap: 0,
                pos: 0,
                lim: 0
            }
        }

        capacity = num::next_power_of_two(capacity);

        let ptr = unsafe { heap::allocate(capacity, mem::min_align_of::<u8>()) };

        ByteBuf {
            ptr: ptr as *mut u8,
            cap: capacity,
            pos: 0,
            lim: capacity
        }
    }

    pub fn capacity(&self) -> uint {
        self.cap
    }

    pub fn flip(&mut self) {
        self.lim = self.pos;
        self.pos = 0;
    }

    pub fn clear(&mut self) {
        self.pos = 0;
        self.lim = self.cap;
    }

    fn as_ptr(&self) -> *const u8 {
        self.ptr as *const u8
    }

    fn as_slice<'a>(&'a self) -> &'a [u8] {
        unsafe {
            mem::transmute(raw::Slice {
                data: self.as_ptr(), len: self.cap
            })
        }
    }

    fn as_mut_slice<'a>(&'a mut self) -> &'a mut [u8] {
        unsafe { mem::transmute(self.as_slice()) }
    }
}

impl Buf for ByteBuf {
    fn remaining(&self) -> uint {
        self.lim - self.pos
    }

    fn bytes<'a>(&'a self) -> &'a [u8] {
        self.as_slice().slice(self.pos, self.lim)
    }

    fn advance(&mut self, mut cnt: uint) {
        cnt = cmp::min(cnt, self.remaining());
        self.pos += cnt;
    }
}

impl MutBuf for ByteBuf {
    fn mut_bytes<'a>(&'a mut self) -> &'a mut [u8] {
        let pos = self.pos;
        let lim = self.lim;
        self.as_mut_slice().mut_slice(pos, lim)
    }
}

impl Reader for ByteBuf {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        super::read(self, buf)
    }
}

impl Writer for ByteBuf {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        super::write(self, buf)
    }
}

#[cfg(test)]
mod test {
    use buf::*;

    #[test]
    pub fn test_initial_buf_empty() {
        let mut buf = ByteBuf::new(100);

        assert!(buf.capacity() == 128);
        assert!(buf.remaining() == 128);

        buf.flip();

        assert!(buf.remaining() == 0);

        buf.clear();

        assert!(buf.remaining() == 128);
    }

    #[test]
    pub fn test_writing_bytes() {
        let mut buf = ByteBuf::new(8);

        buf.write(b"hello").unwrap();
        assert!(buf.remaining() == 3);

        buf.flip();

        assert!(buf.read_to_end().unwrap().as_slice() == b"hello");
    }
}
