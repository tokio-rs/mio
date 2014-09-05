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

    pub fn insert(&mut self, val: T) -> Result<uint, T> {
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
            self.len = idx + 1;
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
    pub fn remove(&mut self, idx: uint) {
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
            fail!("invalid index {} >= {}", idx, self.len);
        }
    }
}

impl<T> Index<uint, T> for Slab<T> {
    fn index<'a>(&'a self, idx: &uint) -> &'a T {
        let idx = *idx;
        self.validate(idx);

        let e = self.entry(idx);

        if e.nxt != 0 {
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

#[cfg(test)]
mod tests {
    use super::Slab;

    #[test]
    fn test_insertion() {
        let mut slab = Slab::new(1);
        let token = slab.insert(10u).ok().expect("Failed to insert");
        assert_eq!(slab[token], 10u);
    }

    #[test]
    fn test_repeated_insertion() {
        let mut slab = Slab::new(10);

        for i in range(0u, 10u) {
            let token = slab.insert(i + 10u).ok().expect("Failed to insert");
            assert_eq!(slab[token], i + 10u);
        }

        slab.insert(20).err().expect("Inserted when full");
    }

    #[test]
    fn test_repeated_insertion_and_removal() {
        let mut slab = Slab::new(10);
        let mut tokens = vec![];

        for i in range(0u, 10u) {
            let token = slab.insert(i + 10u).ok().expect("Failed to insert");
            tokens.push(token);
            assert_eq!(slab[token], i + 10u);
        }

        for &i in tokens.iter() {
            slab.remove(i);
        }

        slab.insert(20).ok().expect("Failed to insert in newly empty slab");
    }

    #[test]
    fn test_insertion_when_full() {
        let mut slab = Slab::new(1);
        slab.insert(10u).ok().expect("Failed to insert");
        slab.insert(10u).err().expect("Inserted into a full slab");
    }

    #[test]
    fn test_removal_is_successful() {
        let mut slab = Slab::new(1);
        let t1 = slab.insert(10u).ok().expect("Failed to insert");
        slab.remove(t1);
        let t2 = slab.insert(20u).ok().expect("Failed to insert");
        assert_eq!(slab[t2], 20u);
    }
}
