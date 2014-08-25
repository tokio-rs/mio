pub use self::slab::Slab;

mod slab {
    use alloc::heap;
    use std::{mem, ptr};

    pub struct Slab<T> {
        mem: *mut Entry<T>,
        len: uint,
        cap: uint,
        nxt: uint, // Next available slot
    }

    impl<T> Slab<T> {
        pub fn new(cap: uint) -> Slab<T> {
            let size = cap.checked_mul(&mem::size_of::<Entry<T>>())
                .expect("capacity overflow");

            let ptr = unsafe { heap::allocate(size, mem::min_align_of::<Entry<T>>()) };

            Slab {
                mem: ptr as *mut Entry<T>,
                cap: cap,
                len: 0,
                nxt: 0,
            }
        }

        pub fn push(&mut self, val: T) -> Result<uint, T> {
            let idx = self.nxt;

            if idx == self.len {
                // No more capacity
                if idx == self.cap {
                    return Err(val);
                }

                {
                    let entry = self.mut_entry(idx);
                    entry.nxt = 0; // Mark as in use
                    entry.val = val;
                }

                self.nxt = idx + 1;
                self.len = idx;
                Ok(idx)
            }
            else {
                let entry = self.mut_entry(idx);
                entry.nxt = 0; // Mark as in use
                entry.val = val;

                Ok(idx)
            }
        }

        /// Releases the given slot
        pub fn release(&mut self, idx: uint) {
            self.validate(idx);

            // Release the value at the entry
            self.release_entry(idx);

            let old_nxt = self.nxt;
            self.nxt = idx;

            let entry = self.mut_entry(idx);
            entry.nxt = old_nxt;
        }

        #[inline]
        fn entry(&self, idx: uint) -> &Entry<T> {
            unsafe { &*self.mem.offset(idx as int) }
        }

        #[inline]
        fn mut_entry(&mut self, idx: uint) -> &mut Entry<T> {
            unsafe { &mut *self.mem.offset(idx as int) }
        }

        #[inline]
        fn release_entry(&mut self, idx: uint) {
            unsafe {
                ptr::read(self.mem.offset(idx as int) as *const Entry<T>);
            }
        }

        #[inline]
        fn validate(&self, idx: uint) {
            if idx >= self.len {
                fail!("invalid index");
            }
        }
    }

    impl<T> Index<uint, T> for Slab<T> {
        fn index<'a>(&'a self, idx: &uint) -> &'a T {
            let idx = *idx;
            self.validate(idx);

            let e = self.entry(idx);

            if e.nxt == 0 {
                fail!("invalid index");
            }

            &e.val
        }
    }

    impl<T> IndexMut<uint, T> for Slab<T> {
        fn index_mut<'a>(&'a mut self, idx: &uint) -> &'a mut T {
            let idx = *idx;
            self.validate(idx);

            let e = self.mut_entry(idx);

            if e.nxt == 0 {
                fail!("invalid index");
            }

            &mut e.val
        }
    }

    #[unsafe_destructor]
    impl<T> Drop for Slab<T> {
        fn drop(&mut self) {
            let mut i = 0;

            while i < self.len {
                if self.entry(i).nxt == 0 {
                    self.release_entry(i);
                }

                i += 1;
            }
        }
    }

    struct Entry<T> {
        nxt: uint, // Next available slot when available, 0 when in use
        val: T // Value at slot
    }
}
