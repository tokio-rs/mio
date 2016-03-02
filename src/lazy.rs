use std::cell::UnsafeCell;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Lazy<T> {
    val: UnsafeCell<Option<T>>,
}

pub struct AtomicLazy<T> {
    val: UnsafeCell<Option<T>>,
    state: AtomicUsize,
}

impl<T> Lazy<T> {
    pub fn new() -> Lazy<T> {
        Lazy { val: UnsafeCell::new(None) }
    }

    pub fn is_some(&self) -> bool {
        self.as_ref().is_some()
    }

    pub fn as_ref(&self) -> Option<&T> {
        self.val().as_ref()
    }

    pub fn set(&self, val: T) -> Result<(), T> {
        if self.is_some() {
            return Err(val);
        }

        unsafe { *self.val.get() = Some(val) }

        Ok(())
    }

    fn val(&self) -> &Option<T> {
        unsafe { &*self.val.get() }
    }
}

impl<T: fmt::Debug> fmt::Debug for Lazy<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("AtomicLazy")
            .field("val", self.val())
            .finish()
    }
}

const NONE: usize = 0;
const LOCK: usize = 1;
const SOME: usize = 2;

impl<T> AtomicLazy<T> {
    pub fn new() -> AtomicLazy<T> {
        AtomicLazy {
            val: UnsafeCell::new(None),
            state: AtomicUsize::new(NONE),
        }
    }

    pub fn as_ref(&self) -> Option<&T> {
        match self.state.load(Ordering::Acquire) {
            SOME => self.val().as_ref(),
            _ => None,
        }
    }

    pub fn set(&self, val: T) -> Result<(), T> {
        if NONE != self.state.compare_and_swap(NONE, LOCK, Ordering::Acquire) {
            return Err(val);
        }

        unsafe { *self.val.get() = Some(val) };

        if LOCK != self.state.compare_and_swap(LOCK, SOME, Ordering::Release) {
            panic!("unable to release lock");
        }

        Ok(())
    }

    fn val(&self) -> &Option<T> {
        unsafe { &*self.val.get() }
    }
}

unsafe impl<T: Sync> Sync for AtomicLazy<T> { }
unsafe impl<T: Send> Send for AtomicLazy<T> { }
