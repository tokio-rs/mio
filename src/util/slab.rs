use std::{mem, ptr, isize};
use std::num::Int;
use std::ops::{Index, IndexMut};
use alloc::heap;
use os::token::Token;

/// A preallocated chunk of memory for storing objects of the same type.
pub struct Slab<T> {
    // Chunk of memory
    mem: *mut Entry<T>,
    // Number of elements currently in the slab
    len: isize,
    // The total number of elements that the slab can hold
    cap: isize,
    // THe token offset
    off: usize,
    // Offset of the next available slot in the slab. Set to the slab's
    // capacity when the slab is full.
    nxt: isize,
    // The total number of slots that were initialized
    init: isize,
}

const MAX: usize = isize::MAX as usize;

// When Entry.nxt is set to this, the entry is in use
const IN_USE: isize = -1;

unsafe impl<T> Send for Slab<T> where T: Send {}

impl<T> Slab<T> {
    pub fn new(cap: usize) -> Slab<T> {
        Slab::new_starting_at(Token(0), cap)
    }

    pub fn new_starting_at(offset: Token, cap: usize) -> Slab<T> {
        assert!(cap <= MAX, "capacity too large");
        // TODO:
        // - Rename to with_capacity
        // - Use a power of 2 capacity
        // - Ensure that mem size is less than usize::MAX

        let size = cap.checked_mul(mem::size_of::<Entry<T>>())
            .expect("capacity overflow");

        let ptr = unsafe { heap::allocate(size, mem::min_align_of::<Entry<T>>()) };

        Slab {
            mem: ptr as *mut Entry<T>,
            cap: cap as isize,
            len: 0,
            off: offset.as_usize(),
            nxt: 0,
            init: 0,
        }
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        (self.cap - self.len) as usize
    }

    #[inline]
    pub fn has_remaining(&self) -> bool {
        self.remaining() > 0
    }

    #[inline]
    pub fn contains(&self, idx: Token) -> bool {
        let idx = self.token_to_idx(idx);

        if idx <= MAX {
            let idx = idx as isize;

            if idx < self.init {
                return self.entry(idx).in_use();
            }
        }

        false
    }

    pub fn get(&self, idx: Token) -> Option<&T> {
        let idx = self.token_to_idx(idx);

        if idx <= MAX {
            let idx = idx as isize;

            if idx < self.init {
                let entry = self.entry(idx);

                if entry.in_use() {
                    return Some(&entry.val);
                }
            }
        }

        None
    }

    pub fn get_mut(&mut self, idx: Token) -> Option<&mut T> {
        let idx = self.token_to_idx(idx);

        if idx <= MAX {
            let idx = idx as isize;

            if idx < self.init {
                let mut entry = self.mut_entry(idx);

                if entry.in_use() {
                    return Some(&mut entry.val);
                }
            }
        }

        None
    }

    pub fn insert(&mut self, val: T) -> Result<Token, T> {
        let idx = self.nxt;

        if idx == self.init {
            // Using an uninitialized entry
            if idx == self.cap {
                // No more capacity
                debug!("slab out of capacity; cap={}", self.cap);
                return Err(val);
            }

            self.mut_entry(idx).put(val, true);

            self.init += 1;
            self.len = self.init;
            self.nxt = self.init;

            debug!("inserting into new slot; idx={}", idx);
        }
        else {
            self.len += 1;
            self.nxt = self.mut_entry(idx).put(val, false);

            debug!("inserting into reused slot; idx={}", idx);
        }

        Ok(self.idx_to_token(idx))
    }

    /// Releases the given slot
    pub fn remove(&mut self, idx: Token) -> Option<T> {
        debug!("removing value; idx={:?}", idx);

        // Cast to usize
        let idx = self.token_to_idx(idx);

        if idx > MAX {
            return None;
        }

        let idx = idx as isize;

        // Ensure index is within capacity of slab
        if idx >= self.init {
            return None;
        }

        let nxt = self.nxt;

        match self.mut_entry(idx).remove(nxt) {
            Some(v) => {
                self.nxt = idx;
                self.len -= 1;
                Some(v)
            }
            None => None
        }
    }

    #[inline]
    fn entry(&self, idx: isize) -> &Entry<T> {
        unsafe { &*self.mem.offset(idx) }
    }

    #[inline]
    fn mut_entry(&mut self, idx: isize) -> &mut Entry<T> {
        unsafe { &mut *self.mem.offset(idx) }
    }

    #[inline]
    fn validate_idx(&self, idx: usize) -> isize {
        if idx <= MAX {
            let idx = idx as isize;

            if idx < self.init {
                return idx;
            }
        }

        panic!("invalid index {} -- greater than capacity {}", idx, self.cap);
    }

    fn token_to_idx(&self, token: Token) -> usize {
        token.as_usize() - self.off
    }

    fn idx_to_token(&self, idx: isize) -> Token {
        Token(idx as usize + self.off)
    }
}

impl<T> Index<Token> for Slab<T> {
    type Output = T;

    fn index<'a>(&'a self, idx: &Token) -> &'a T {
        let idx = self.token_to_idx(*idx);
        let idx = self.validate_idx(idx);

        let e = self.entry(idx);

        if !e.in_use() {
            panic!("invalid index; idx={}", idx);
        }

        &e.val
    }
}

impl<T> IndexMut<Token> for Slab<T> {
    fn index_mut<'a>(&'a mut self, idx: &Token) -> &'a mut T {
        let idx = self.token_to_idx(*idx);
        let idx = self.validate_idx(idx);

        let e = self.mut_entry(idx);

        if !e.in_use() {
            panic!("invalid index; idx={}", idx);
        }

        &mut e.val
    }
}

#[unsafe_destructor]
impl<T> Drop for Slab<T> {
    fn drop(&mut self) {
        // TODO: check whether or not this is needed with intrinsics::needs_drop
        let mut i = 0;

        while i < self.init {
            self.mut_entry(i).release();
            i += 1;
        }

        let cap = self.cap as usize;
        let size = cap.checked_mul(mem::size_of::<Entry<T>>()).unwrap();
        unsafe { heap::deallocate(self.mem as *mut u8, size, mem::min_align_of::<Entry<T>>()) };
    }
}

// Holds the values in the slab.
struct Entry<T> {
    nxt: isize,
    val: T
}

impl<T> Entry<T> {
    #[inline]
    fn put(&mut self, val: T, init: bool) -> isize {
        assert!(init || self.nxt != IN_USE);

        let ret = self.nxt;

        unsafe { ptr::write(&mut self.val as *mut T, val); }
        self.nxt = IN_USE;

        // Could be uninitialized memory, but the caller (Slab) should guard
        // not use the return value in those cases.
        ret
    }

    fn remove(&mut self, nxt: isize) -> Option<T> {
        if self.in_use() {
            self.nxt = nxt;
            Some(unsafe { ptr::read(&self.val as *const T) })
        } else {
            None
        }
    }

    fn release(&mut self) {
        if self.in_use() {
            let _ = Some(unsafe { ptr::read(&self.val as *const T) });
        }
    }

    #[inline]
    fn in_use(&self) -> bool {
        self.nxt == IN_USE
    }
}

#[cfg(test)]
mod tests {
    use super::Slab;
    use {Token};

    #[test]
    fn test_insertion() {
        let mut slab = Slab::new(1);
        let token = slab.insert(10).ok().expect("Failed to insert");
        assert_eq!(slab[token], 10);
    }

    #[test]
    fn test_repeated_insertion() {
        let mut slab = Slab::new(10);

        for i in range(0, 10) {
            let token = slab.insert(i + 10).ok().expect("Failed to insert");
            assert_eq!(slab[token], i + 10);
        }

        slab.insert(20).err().expect("Inserted when full");
    }

    #[test]
    fn test_repeated_insertion_and_removal() {
        let mut slab = Slab::new(10);
        let mut tokens = vec![];

        for i in range(0, 10) {
            let token = slab.insert(i + 10).ok().expect("Failed to insert");
            tokens.push(token);
            assert_eq!(slab[token], i + 10);
        }

        for &i in tokens.iter() {
            slab.remove(i);
        }

        slab.insert(20).ok().expect("Failed to insert in newly empty slab");
    }

    #[test]
    fn test_insertion_when_full() {
        let mut slab = Slab::new(1);
        slab.insert(10).ok().expect("Failed to insert");
        slab.insert(10).err().expect("Inserted into a full slab");
    }

    #[test]
    fn test_removal_is_successful() {
        let mut slab = Slab::new(1);
        let t1 = slab.insert(10).ok().expect("Failed to insert");
        slab.remove(t1);
        let t2 = slab.insert(20).ok().expect("Failed to insert");
        assert_eq!(slab[t2], 20);
    }

    #[test]
    fn test_mut_retrieval() {
        let mut slab = Slab::new(1);
        let t1 = slab.insert("foo".to_string()).ok().expect("Failed to insert");

        slab[t1].push_str("bar");

        assert_eq!(slab[t1].as_slice(), "foobar");
    }

    #[test]
    #[should_fail]
    fn test_reusing_slots_1() {
        let mut slab = Slab::new(16);

        let t0 = slab.insert(123).unwrap();
        let t1 = slab.insert(456).unwrap();

        assert!(slab.count() == 2);
        assert!(slab.remaining() == 14);

        slab.remove(t0);

        assert!(slab.count() == 1, "actual={}", slab.count());
        assert!(slab.remaining() == 15);

        slab.remove(t1);

        assert!(slab.count() == 0);
        assert!(slab.remaining() == 16);

        let _ = slab[t1];
    }

    #[test]
    fn test_reusing_slots_2() {
        let mut slab = Slab::new(16);

        let t0 = slab.insert(123).unwrap();

        assert!(slab[t0] == 123);
        assert!(slab.remove(t0) == Some(123));

        let t0 = slab.insert(456).unwrap();

        assert!(slab[t0] == 456);

        let t1 = slab.insert(789).unwrap();

        assert!(slab[t0] == 456);
        assert!(slab[t1] == 789);

        assert!(slab.remove(t0).unwrap() == 456);
        assert!(slab.remove(t1).unwrap() == 789);

        assert!(slab.count() == 0);
    }

    #[test]
    #[should_fail]
    fn test_accessing_out_of_bounds() {
        let slab = Slab::<usize>::new(16);
        slab[Token(0)];
    }

    #[test]
    fn test_contains() {
        let mut slab = Slab::new_starting_at(Token(5),16);
        assert!(!slab.contains(Token(0)));

        let tok = slab.insert(111).unwrap();
        assert!(slab.contains(tok));
    }
}
