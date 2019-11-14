use ntapi::ntioapi::{IO_STATUS_BLOCK_u, IO_STATUS_BLOCK};
use std::fmt;
use std::ops::{Deref, DerefMut};
use winapi::shared::ntdef::NTSTATUS;

pub struct IoStatusBlock(IO_STATUS_BLOCK);

impl IoStatusBlock {
    pub fn zeroed() -> Self {
        Self(IO_STATUS_BLOCK {
            u: IO_STATUS_BLOCK_u { Status: 0 },
            Information: 0,
        })
    }

    pub fn status(&self) -> NTSTATUS {
        unsafe { self.u.Status }
    }
}

unsafe impl Send for IoStatusBlock {}

impl Deref for IoStatusBlock {
    type Target = IO_STATUS_BLOCK;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for IoStatusBlock {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Debug for IoStatusBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IoStatusBlock").finish()
    }
}
