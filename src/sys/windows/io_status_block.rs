use ntapi::ntioapi::{IO_STATUS_BLOCK_u, IO_STATUS_BLOCK};
use std::cell::UnsafeCell;
use std::fmt;

pub struct IoStatusBlock(UnsafeCell<IO_STATUS_BLOCK>);

// There is a pointer field in `IO_STATUS_BLOCK_u`, which we don't use that. Thus it is safe to implement Send here.
unsafe impl Send for IoStatusBlock {}

impl IoStatusBlock {
    pub fn zeroed() -> IoStatusBlock {
        let iosb = IO_STATUS_BLOCK {
            u: IO_STATUS_BLOCK_u { Status: 0 },
            Information: 0,
        };
        IoStatusBlock(UnsafeCell::new(iosb))
    }

    pub fn as_ptr(&self) -> *const IO_STATUS_BLOCK {
        self.0.get()
    }

    pub fn as_mut_ptr(&self) -> *mut IO_STATUS_BLOCK {
        self.0.get()
    }
}

impl fmt::Debug for IoStatusBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IoStatusBlock").finish()
    }
}
