use std::slice::bytes;
use error::{MioResult, MioError};

pub trait Buf {
    fn remaining(&self) -> uint;
    fn as_slice<'a>(&'a self) -> &'a [u8];
    fn advance(&mut self, cnt: uint);

    /// Gets the current byte and advances the cursor
    fn get(&mut self) -> MioResult<u8> {
        if self.remaining() == 0 {
            return Err(MioError::buf_underflow());
        }

        let b = self.as_slice()[0];
        self.advance(1);
        Ok(b)
    }
}

pub trait MutBuf : Buf {
    fn as_mut_slice<'a>(&'a mut self) -> &'a mut [u8];

    fn put(&mut self, b: u8) -> MioResult<()> {
        let s = self.as_mut_slice();

        if s.len() > 0 {
            s[0] = b;
            return Ok(());
        }

        Err(MioError::buf_overflow())
    }

    fn put_bytes(&mut self, mut b: &[u8]) -> MioResult<()> {
        if self.remaining() < b.len() {
            return Err(MioError::buf_overflow());
        }

        while !b.is_empty() {
            let s = self.as_mut_slice();
            let l = s.len();

            // Must be greater than zero since the remaining capacity has been
            // checked.
            assert!(l > 0);
            bytes::copy_memory(s, b.slice_to(l));

            // Track what is remaining
            b = b.slice_from(l);
        }

        Ok(())
    }
}

mod ring {
    use std::{mem, ptr};
    use std::raw::Slice;
    use alloc::heap;
    use super::{Buf, MutBuf};

    /// Buf backed by a continous chunk of memory. Maintains a read cursor and a
    /// write cursor. When reads and writes reach the end of the allocated buffer,
    /// wraps around to the start.
    pub struct RingBuf {
        ptr: *mut u8,   // Pointer to the memory
        cap: uint,      // Capacity of the buffer
        read: uint,     // Read cursor
        write: uint     // Write cursor
    }

    // When read == write, is the buf empty or full?
    impl RingBuf {
        pub fn new(capacity: uint) -> RingBuf {
            // Handle the 0 length buffer case
            if capacity == 0 {
                return RingBuf {
                    ptr: ptr::mut_null(),
                    cap: 0,
                    read: 0,
                    write: 0
                }
            }

            // Allocate the memory
            let ptr = unsafe { heap::allocate(capacity, mem::min_align_of::<u8>()) };

            RingBuf {
                ptr: ptr as *mut u8,
                cap: capacity,
                read: 0,
                write: 0
            }
        }

        pub fn capacity(&self) -> uint {
            self.cap
        }

        // Access readable bytes as a Buf
        pub fn reader<'a>(&'a mut self) -> RingBufReader<'a> {
            RingBufReader { ring: self }
        }

        // Access writable bytes as a Buf
        pub fn writer<'a>(&'a mut self) -> RingBufWriter<'a> {
            RingBufWriter { ring: self }
        }

        fn read_to(&self) -> uint {
            if self.write >= self.read {
                self.write
            } else {
                self.cap
            }
        }

        fn advance_reader(&mut self, cnt: uint) {
            assert!(cnt < self.cap);

            if self.write >= self.read {
                // Can only advance up to self.write
                self.read += cnt;

                if self.read > self.write {
                    self.read = self.write;
                }
            } else {
                // self.write has already wrapped around
                self.read += cnt;

                if self.read >= self.cap {
                    // If advanced pass the capacity of the buffer, wrap around
                    self.read %= self.cap;

                    // Since a wrap around happened, check that the reader has
                    // not passed the writer.
                    if self.read > self.write {
                        self.read = self.write;
                    }
                }
            }
        }

        fn advance_writer(&mut self, cnt: uint) {
            assert!(cnt < self.cap);

            if self.read >= self.write {
                // Can only advance up to self.read
                self.write += cnt;

                if self.write > self.read {
                    self.write = self.read;
                }
            } else {
                self.write += cnt;

                if self.write >= self.cap {
                    self.write %= self.cap;

                    if self.write > self.read {
                        self.write = self.read;
                    }
                }
            }
        }

        fn write_to(&self) -> uint {
            if self.read >= self.write {
                self.read
            } else {
                self.cap
            }
        }

        fn as_ptr(&self) -> *const u8 {
            self.ptr as *const u8
        }

        fn as_slice<'a>(&'a self) -> &'a [u8] {
            unsafe {
                mem::transmute(Slice {
                    data: self.as_ptr(), len: self.cap
                })
            }
        }

        fn as_mut_slice<'a>(&'a mut self) -> &'a mut [u8] {
            unsafe {
                mem::transmute(Slice {
                    data: self.as_ptr(), len: self.cap
                })
            }
        }
    }

    impl Clone for RingBuf {
        fn clone(&self) -> RingBuf {
            let mut ret = RingBuf::new(self.cap);

            ret.read = self.read;
            ret.write = self.write;

            unsafe {
                if self.write >= self.read {
                    // Copy the data that is live
                    ptr::copy_memory(
                        ret.ptr.offset(self.read as int),
                        self.ptr.offset(self.read as int) as *const u8,
                        self.write - self.read);
                } else {
                    // The write cursor has wrapped around
                    // First, write the first half
                    ptr::copy_memory(ret.ptr, self.ptr as *const u8, self.write);
                    // Write the second half
                    ptr::copy_memory(
                        ret.ptr.offset(self.read as int),
                        self.ptr.offset(self.read as int) as *const u8,
                        self.cap - self.read);
                }
            }

            ret
        }

        // TODO: an improved version of clone_from is possible.
    }

    impl Drop for RingBuf {
        fn drop(&mut self) {
            if self.cap > 0 {
                unsafe {
                    heap::deallocate(self.ptr, self.cap, mem::min_align_of::<u8>())
                }
            }
        }
    }

    pub struct RingBufReader<'a> {
        ring: &'a mut RingBuf
    }

    impl<'a> Buf for RingBufReader<'a> {
        fn remaining(&self) -> uint {
            self.ring.write - self.ring.read
        }

        fn as_slice<'a>(&'a self) -> &'a [u8] {
            self.ring.as_slice()
                .slice(self.ring.read, self.ring.read_to())
        }

        fn advance(&mut self, cnt: uint) {
            self.ring.advance_reader(cnt)
        }
    }

    pub struct RingBufWriter<'a> {
        ring: &'a mut RingBuf
    }

    impl<'a> Buf for RingBufWriter<'a> {
        fn remaining(&self) -> uint {
            self.ring.cap - (self.ring.write - self.ring.read)
        }

        fn as_slice<'a>(&'a self) -> &'a [u8] {
            self.ring.as_slice()
                .slice(self.ring.write, self.ring.write_to())
        }

        fn advance(&mut self, cnt: uint) {
            self.ring.advance_writer(cnt)
        }
    }

    impl<'a> MutBuf for RingBufWriter<'a> {
        fn as_mut_slice<'a>(&'a mut self) -> &'a mut [u8] {
            let from = self.ring.write;
            let to = self.ring.write_to();

            self.ring.as_mut_slice().mut_slice(from, to)
        }
    }

    #[test]
    pub fn test_initial_read_buf_empty() {
        let mut buf = RingBuf::new(100);
        assert!(buf.reader().as_slice().is_empty());
    }

    #[test]
    pub fn test_using_ring_buffer() {
        let mut buf = RingBuf::new(100);

    }
}
