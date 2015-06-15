use std::{fmt, mem, usize};
use std::iter::IntoIterator;
use std::ops;

/// A preallocated chunk of memory for storing objects of the same type.
pub struct Slab<T, I : Index> {
    // Chunk of memory
    entries: Vec<Entry<T>>,
    // Number of elements currently in the slab
    len: usize,
    // The index offset
    off: I,
    // Offset of the next available slot in the slab. Set to the slab's
    // capacity when the slab is full.
    nxt: usize,
}

/// Slab can be indexed by any type implementing `Index` trait.
pub trait Index {
    fn from_usize(i : usize) -> Self;
    fn as_usize(&self) -> usize;
}

impl Index for usize {
    fn from_usize(i : usize) -> usize {
        i
    }

    fn as_usize(&self) -> usize {
        *self
    }
}

const MAX: usize = usize::MAX;

unsafe impl<T, I : Index> Send for Slab<T, I> where T: Send {}

// TODO: Once NonZero lands, use it to optimize the layout
impl<T, I : Index> Slab<T, I> {
    pub fn new(cap: usize) -> Slab<T, I> {
        Slab::new_starting_at(I::from_usize(0), cap)
    }

    pub fn new_starting_at(offset: I, cap: usize) -> Slab<T, I> {
        assert!(cap <= MAX, "capacity too large");
        // TODO:
        // - Rename to with_capacity
        // - Use a power of 2 capacity
        // - Ensure that mem size is less than usize::MAX

        let entries = Vec::with_capacity(cap);

        Slab {
            entries: entries,
            len: 0,
            off: offset,
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
    pub fn contains(&self, idx : I) -> bool {
        if idx.as_usize() < self.off.as_usize() {
            return false;
        }

        let idx = self.global_to_local_idx(idx);

        if idx < self.entries.len() {
            return self.entries[idx].in_use();
        }

        false
    }

    pub fn get(&self, idx: I) -> Option<&T> {
        let idx = self.global_to_local_idx(idx);

        if idx <= MAX {
            if idx < self.entries.len() {
                return self.entries[idx].val.as_ref();
            }
        }

        None
    }

    pub fn get_mut(&mut self, idx: I) -> Option<&mut T> {
        let idx = self.global_to_local_idx(idx);

        if idx <= MAX {
            if idx < self.entries.len() {
                return self.entries[idx].val.as_mut();
            }
        }

        None
    }

    pub fn insert(&mut self, val: T) -> Result<I, T> {
        let idx = self.nxt;
        // check fail condition before val gets moved by insert_with,
        // so `Err(val)` can be returned
        if idx == self.entries.capacity() {
            Err(val)
        } else {
            match self.insert_with(move |_| val ) {
                None => panic!("Slab::insert_with() should"),
                Some(idx) => Ok(idx)
            }
        }
    }

    /// Like `insert` but for objects that require newly allocated
    /// usize in their constructor.
    pub fn insert_with<F>(&mut self, f : F) -> Option<I>
    where F : FnOnce(I) -> T {
        let idx = self.nxt;

        if idx == self.entries.len() {
            // Using an uninitialized entry
            if idx == self.entries.capacity() {
                // No more capacity
                debug!("slab out of capacity; cap={}", self.entries.capacity());
                return None;
            }

            let val = f(self.local_to_global_idx(idx));
            self.entries.push(Entry {
                nxt: MAX,
                val: Some(val),
            });

            self.len += 1;
            self.nxt = self.len;
        }
        else {
            let val = f(self.local_to_global_idx(idx));
            self.len += 1;
            self.nxt = self.entries[idx].put(val);
        }

        Some(self.local_to_global_idx(idx))
    }

    /// Releases the given slot
    pub fn remove(&mut self, idx: I) -> Option<T> {
        let idx = self.global_to_local_idx(idx);

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

    pub fn replace(&mut self, idx: I, t : T) -> Option<T> {
        let idx = self.global_to_local_idx(idx);

        if idx > self.entries.len() {
            return None;
        }

        if idx <= MAX {
            if idx < self.entries.len() {
                let val = self.entries[idx].val.as_mut().unwrap();
                return Some(mem::replace(val, t))
            }
        }
        None
    }


    pub fn iter(&self) -> SlabIter<T, I> {
        SlabIter {
            slab: self,
            cur_idx: 0,
            yielded: 0
        }
    }

    pub fn iter_mut(&mut self) -> SlabMutIter<T, I> {
        SlabMutIter { iter: self.iter() }
    }

    #[inline]
    fn validate_idx(&self, idx: usize) -> usize {
        if idx < self.entries.len() {
            return idx;
        }

        panic!("invalid index {} -- greater than capacity {}", idx, self.entries.capacity());
    }

    fn global_to_local_idx(&self, idx: I) -> usize {
        idx.as_usize() - self.off.as_usize()
    }

    fn local_to_global_idx(&self, idx: usize) -> I {
        I::from_usize(idx + self.off.as_usize())
    }
}

impl<T, I : Index> ops::Index<I> for Slab<T, I> {
    type Output = T;

    fn index<'a>(&'a self, idx: I) -> &'a T {
        let idx = self.global_to_local_idx(idx);
        let idx = self.validate_idx(idx);

        self.entries[idx].val.as_ref()
            .expect("invalid index")
    }
}

impl<T, I : Index> ops::IndexMut<I> for Slab<T, I> {
    fn index_mut<'a>(&'a mut self, idx: I) -> &'a mut T {
        let idx = self.global_to_local_idx(idx);
        let idx = self.validate_idx(idx);

        self.entries[idx].val.as_mut()
            .expect("invalid index")
    }
}

impl<T, I : Index> fmt::Debug for Slab<T, I> {
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
    fn put(&mut self, val: T) -> usize {
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

pub struct SlabIter<'a, T: 'a, I : Index+'a> {
    slab: &'a Slab<T, I>,
    cur_idx: usize,
    yielded: usize
}

impl<'a, T, I : Index> Iterator for SlabIter<'a, T, I> {
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

pub struct SlabMutIter<'a, T: 'a, I : Index+'a> {
    iter: SlabIter<'a, T, I>,
}

impl<'a, T, I : Index> Iterator for SlabMutIter<'a, T, I> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        unsafe { mem::transmute(self.iter.next()) }
    }
}

impl<'a, T, I : Index> IntoIterator for &'a Slab<T, I> {
    type Item = &'a T;
    type IntoIter = SlabIter<'a, T, I>;

    fn into_iter(self) -> SlabIter<'a, T, I> {
        self.iter()
    }
}

impl<'a, T, I : Index> IntoIterator for &'a mut Slab<T, I> {
    type Item = &'a mut T;
    type IntoIter = SlabMutIter<'a, T, I>;

    fn into_iter(self) -> SlabMutIter<'a, T, I> {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::Slab;

    #[test]
    fn test_insertion() {
        let mut slab = Slab::<usize, usize>::new(1);
        let idx = slab.insert(10).ok().expect("Failed to insert");
        assert_eq!(slab[idx], 10);
    }

    #[test]
    fn test_repeated_insertion() {
        let mut slab = Slab::<usize, usize>::new(10);

        for i in (0..10) {
            let idx= slab.insert(i + 10).ok().expect("Failed to insert");
            assert_eq!(slab[idx], i + 10);
        }

        slab.insert(20).err().expect("Inserted when full");
    }

    #[test]
    fn test_repeated_insertion_and_removal() {
        let mut slab = Slab::<usize, usize>::new(10);
        let mut indices = vec![];

        for i in 0..10 {
            let idx = slab.insert(i + 10).ok().expect("Failed to insert");
            indices.push(idx);
            assert_eq!(slab[idx], i + 10);
        }

        for &i in indices.iter() {
            slab.remove(i);
        }

        slab.insert(20).ok().expect("Failed to insert in newly empty slab");
    }

    #[test]
    fn test_insertion_when_full() {
        let mut slab = Slab::<usize, usize>::new(1);
        slab.insert(10).ok().expect("Failed to insert");
        slab.insert(10).err().expect("Inserted into a full slab");
    }

    #[test]
    fn test_removal_is_successful() {
        let mut slab = Slab::<usize, usize>::new(1);
        let t1 = slab.insert(10).ok().expect("Failed to insert");
        slab.remove(t1);
        let t2 = slab.insert(20).ok().expect("Failed to insert");
        assert_eq!(slab[t2], 20);
    }

    #[test]
    fn test_mut_retrieval() {
        let mut slab = Slab::<_, usize>::new(1);
        let t1 = slab.insert("foo".to_string()).ok().expect("Failed to insert");

        slab[t1].push_str("bar");

        assert_eq!(&slab[t1][..], "foobar");
    }

    #[test]
    #[should_panic]
    fn test_reusing_slots_1() {
        let mut slab = Slab::<usize, usize>::new(16);

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
        let mut slab = Slab::<usize, usize>::new(16);

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
        let slab = Slab::<usize, usize>::new(16);
        slab[0];
    }

    #[test]
    fn test_contains() {
        let mut slab = Slab::new_starting_at(5 ,16);
        assert!(!slab.contains(0));

        let idx = slab.insert(111).unwrap();
        assert!(slab.contains(idx));
    }

    #[test]
    fn test_iter() {
        let mut slab = Slab::<u32, usize>::new_starting_at(0, 4);
        for i in (0..4) {
            slab.insert(i).unwrap();
        }

        let vals: Vec<u32> = slab.iter().map(|r| *r).collect();
        assert_eq!(vals, vec![0, 1, 2, 3]);

        slab.remove(1);

        let vals: Vec<u32> = slab.iter().map(|r| *r).collect();
        assert_eq!(vals, vec![0, 2, 3]);
    }

    #[test]
    fn test_iter_mut() {
        let mut slab = Slab::<u32, usize>::new_starting_at(0, 4);
        for i in (0..4) {
            slab.insert(i).unwrap();
        }
        for e in slab.iter_mut() {
            *e = *e + 1;
        }

        let vals: Vec<u32> = slab.iter().map(|r| *r).collect();
        assert_eq!(vals, vec![1, 2, 3, 4]);

        slab.remove(2);
        for e in slab.iter_mut() {
            *e = *e + 1;
        }

        let vals: Vec<u32> = slab.iter().map(|r| *r).collect();
        assert_eq!(vals, vec![2, 3, 5]);
    }

    #[test]
    fn test_get() {
        let mut slab = Slab::<usize, usize>::new(16);
        let tok = slab.insert(5).unwrap();
        assert_eq!(slab.get(tok), Some(&5));
        assert_eq!(slab.get(1), None);
        assert_eq!(slab.get(23), None);
    }

    #[test]
    fn test_get_mut() {
        let mut slab = Slab::<u32, usize>::new(16);
        let tok = slab.insert(5u32).unwrap();
        {
            let mut_ref = slab.get_mut(tok).unwrap();
            assert_eq!(*mut_ref, 5);
            *mut_ref = 12;
        }
        assert_eq!(slab[tok], 12);
        assert_eq!(slab.get_mut(1), None);
        assert_eq!(slab.get_mut(23), None);
    }
}
