use std::cell::UnsafeCell;
use std::fmt;
use std::mem;
use std::ops::{Deref, DerefMut};

use ntapi::ntioapi::IO_STATUS_BLOCK;

pub struct IoStatusBlock(UnsafeCell<IO_STATUS_BLOCK>);

unsafe impl Send for IoStatusBlock {}

impl IoStatusBlock {
    pub fn zeroed() -> IoStatusBlock {
        let iosb = unsafe { mem::zeroed::<IO_STATUS_BLOCK>() };
        IoStatusBlock(UnsafeCell::new(iosb))
    }

    pub fn as_ptr(&self) -> *const IO_STATUS_BLOCK {
        self.0.get()
    }

    pub fn as_mut_ptr(&self) -> *mut IO_STATUS_BLOCK {
        self.0.get()
    }
}

impl Deref for IoStatusBlock {
    type Target = IO_STATUS_BLOCK;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.as_ptr() }
    }
}

impl DerefMut for IoStatusBlock {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.as_mut_ptr() }
    }
}

impl fmt::Debug for IoStatusBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IoStatusBlock").finish()
    }
}
