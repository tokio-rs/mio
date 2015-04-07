use std::{fmt, mem, usize};
use std::ops::{Index, IndexMut};
use token::Token;

/// A preallocated chunk of memory for storing objects of the same type.
pub struct Slab<T> {
    // Chunk of memory
    entries: Vec<Entry<T>>,
    // Number of elements currently in the slab
    len: usize,
    // The token offset
    off: usize,
    // Offset of the next available slot in the slab. Set to the slab's
    // capacity when the slab is full.
    nxt: usize,
}

const MAX: usize = usize::MAX;

unsafe impl<T> Send for Slab<T> where T: Send {}

// TODO: Once NonZero lands, use it to optimize the layout
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

        let entries = Vec::with_capacity(cap);

        Slab {
            entries: entries,
            len: 0,
            off: offset.as_usize(),
            nxt: 0,
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
        (self.entries.capacity() - self.len) as usize
    }

    #[inline]
    pub fn has_remaining(&self) -> bool {
        self.remaining() > 0
    }

    #[inline]
    pub fn contains(&self, idx: Token) -> bool {
        if idx.as_usize() < self.off {
            return false;
        }

        let idx = self.token_to_idx(idx);

        if idx < self.entries.len() {
            return self.entries[idx].in_use();
        }

        false
    }

    pub fn get(&self, idx: Token) -> Option<&T> {
        assert!(self.contains(idx), "slab does not contain token `{:?}`", idx);

        let idx = self.token_to_idx(idx);

        if idx <= MAX {
            if idx < self.entries.len() {
                return self.entries[idx].val.as_ref();
            }
        }

        None
    }

    pub fn get_mut(&mut self, idx: Token) -> Option<&mut T> {
        let idx = self.token_to_idx(idx);

        if idx <= MAX {
            if idx < self.entries.len() {
                return self.entries[idx].val.as_mut();
            }
        }

        None
    }

    pub fn insert(&mut self, val: T) -> Result<Token, T> {
        let idx = self.nxt;

        if idx == self.entries.len() {
            // Using an uninitialized entry
            if idx == self.entries.capacity() {
                // No more capacity
                debug!("slab out of capacity; cap={}", self.entries.capacity());
                return Err(val);
            }

            self.entries.push(Entry {
                nxt: MAX,
                val: Some(val),
            });

            self.len += 1;
            self.nxt = self.len;
        }
        else {
            self.len += 1;
            self.nxt = self.entries[idx].put(val);
        }

        Ok(self.idx_to_token(idx))
    }

    /// Releases the given slot
    pub fn remove(&mut self, idx: Token) -> Option<T> {
        // Cast to usize
        let idx = self.token_to_idx(idx);

        if idx > self.entries.len() {
            return None;
        }

        match self.entries[idx].remove(self.nxt) {
            Some(v) => {
                self.nxt = idx;
                self.len -= 1;
                Some(v)
            }
            None => None
        }
    }

    pub fn iter(&self) -> SlabIter<T> {
        SlabIter {
            slab: self,
            cur_idx: 0,
            yielded: 0
        }
    }

    pub fn iter_mut(&mut self) -> SlabMutIter<T> {
        SlabMutIter { iter: self.iter() }
    }

    #[inline]
    fn validate_idx(&self, idx: usize) -> usize {
        if idx < self.entries.len() {
            return idx;
        }

        panic!("invalid index {} -- greater than capacity {}", idx, self.entries.capacity());
    }

    fn token_to_idx(&self, token: Token) -> usize {
        token.as_usize() - self.off
    }

    fn idx_to_token(&self, idx: usize) -> Token {
        Token(idx as usize + self.off)
    }
}

impl<T> Index<Token> for Slab<T> {
    type Output = T;

    fn index<'a>(&'a self, idx: Token) -> &'a T {
        let idx = self.token_to_idx(idx);
        let idx = self.validate_idx(idx);

        self.entries[idx].val.as_ref()
            .expect("invalid index")
    }
}

impl<T> IndexMut<Token> for Slab<T> {
    fn index_mut<'a>(&'a mut self, idx: Token) -> &'a mut T {
        let idx = self.token_to_idx(idx);
        let idx = self.validate_idx(idx);

        self.entries[idx].val.as_mut()
            .expect("invalid index")
    }
}

impl<T> fmt::Debug for Slab<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Slab {{ len: {}, cap: {} }}", self.len, self.entries.capacity())
    }
}

// Holds the values in the slab.
struct Entry<T> {
    nxt: usize,
    val: Option<T>,
}

impl<T> Entry<T> {
    #[inline]
    fn put(&mut self, val: T) -> usize{
        let ret = self.nxt;
        self.val = Some(val);
        ret
    }

    fn remove(&mut self, nxt: usize) -> Option<T> {
        if self.in_use() {
            self.nxt = nxt;
            self.val.take()
        } else {
            None
        }
    }

    #[inline]
    fn in_use(&self) -> bool {
        self.val.is_some()
    }
}

pub struct SlabIter<'a, T: 'a> {
    slab: &'a Slab<T>,
    cur_idx: usize,
    yielded: usize
}

impl<'a, T> Iterator for SlabIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        while self.yielded < self.slab.len {
            match self.slab.entries[self.cur_idx].val {
                Some(ref v) => {
                    self.cur_idx += 1;
                    self.yielded += 1;
                    return Some(v);
                }
                None => {
                    self.cur_idx += 1;
                }
            }
        }

        None
    }
}

pub struct SlabMutIter<'a, T: 'a> {
    iter: SlabIter<'a, T>,
}

impl<'a, 'b, T> Iterator for SlabMutIter<'a, T> {
    type Item = &'b mut T;

    fn next(&mut self) -> Option<&'b mut T> {
        unsafe { mem::transmute(self.iter.next()) }
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

        for i in (0..10) {
            let token = slab.insert(i + 10).ok().expect("Failed to insert");
            assert_eq!(slab[token], i + 10);
        }

        slab.insert(20).err().expect("Inserted when full");
    }

    #[test]
    fn test_repeated_insertion_and_removal() {
        let mut slab = Slab::new(10);
        let mut tokens = vec![];

        for i in (0..10) {
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

        assert_eq!(&slab[t1][..], "foobar");
    }

    #[test]
    #[should_panic]
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
    #[should_panic]
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

    #[test]
    fn test_iter() {
        let mut slab: Slab<u32> = Slab::new_starting_at(Token(0), 4);
        for i in (0..4) {
            slab.insert(i).unwrap();
        }

        let vals: Vec<u32> = slab.iter().map(|r| *r).collect();
        assert_eq!(vals, vec![0, 1, 2, 3]);

        slab.remove(Token(1));

        let vals: Vec<u32> = slab.iter().map(|r| *r).collect();
        assert_eq!(vals, vec![0, 2, 3]);
    }

    #[test]
    fn test_iter_mut() {
        let mut slab: Slab<u32> = Slab::new_starting_at(Token(0), 4);
        for i in (0..4) {
            slab.insert(i).unwrap();
        }
        for e in slab.iter_mut() {
            *e = *e + 1;
        }

        let vals: Vec<u32> = slab.iter().map(|r| *r).collect();
        assert_eq!(vals, vec![1, 2, 3, 4]);

        slab.remove(Token(2));
        for e in slab.iter_mut() {
            *e = *e + 1;
        }

        let vals: Vec<u32> = slab.iter().map(|r| *r).collect();
        assert_eq!(vals, vec![2, 3, 5]);
    }
}
