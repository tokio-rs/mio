// Original work Copyright (c) 2014 The Rust Project Developers
// Modified work Copyright (c) 2016-2018 Nikita Pekin and the lazycell contributors
// See the README.md file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

// Tracks the AtomicLazyCell inner state
const NONE: usize = 0;
const LOCK: usize = 1;
const SOME: usize = 2;

/// A lazily filled and thread-safe `Cell`, with frozen contents.
#[derive(Debug, Default)]
pub(crate) struct AtomicLazyCell<T> {
    inner: UnsafeCell<Option<T>>,
    state: AtomicUsize,
}

impl<T> AtomicLazyCell<T> {
    /// Creates a new, empty, `AtomicLazyCell`.
    pub fn new() -> AtomicLazyCell<T> {
        Self {
            inner: UnsafeCell::new(None),
            state: AtomicUsize::new(NONE),
        }
    }

    /// Put a value into this cell.
    ///
    /// This function will return `Err(value)` is the cell is already full.
    pub fn fill(&self, t: T) -> Result<(), T> {
        if NONE != self.state.compare_and_swap(NONE, LOCK, Ordering::Acquire) {
            return Err(t);
        }

        unsafe { *self.inner.get() = Some(t) };

        if LOCK != self.state.compare_and_swap(LOCK, SOME, Ordering::Release) {
            panic!("unable to release lock");
        }

        Ok(())
    }

    /// Borrows the contents of this lazy cell for the duration of the cell
    /// itself.
    ///
    /// This function will return `Some` if the cell has been previously
    /// initialized, and `None` if it has not yet been initialized.
    pub fn borrow(&self) -> Option<&T> {
        match self.state.load(Ordering::Acquire) {
            SOME => unsafe { &*self.inner.get() }.as_ref(),
            _ => None,
        }
    }
}

unsafe impl<T: Sync + Send> Sync for AtomicLazyCell<T> {}
unsafe impl<T: Send> Send for AtomicLazyCell<T> {}
