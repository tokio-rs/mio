use crate::sys::windows::Event;

use std::cell::UnsafeCell;
use std::fmt;

use winapi::um::minwinbase::OVERLAPPED_ENTRY;
#[cfg(feature = "os-util")]
use winapi::um::minwinbase::OVERLAPPED;

#[repr(C)]
pub(crate) struct Overlapped {
    inner: UnsafeCell<miow::Overlapped>,
    pub(crate) callback: fn(&OVERLAPPED_ENTRY, Option<&mut Vec<Event>>),
}

#[cfg(feature = "os-util")]
impl Overlapped {
    pub(crate) fn new(cb: fn(&OVERLAPPED_ENTRY, Option<&mut Vec<Event>>)) -> Overlapped {
        Overlapped {
            inner: UnsafeCell::new(miow::Overlapped::zero()),
            callback: cb,
        }
    }

    pub(crate) fn as_ptr(&self) -> *const OVERLAPPED {
        unsafe { (*self.inner.get()).raw() }
    }
}

impl fmt::Debug for Overlapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Overlapped").finish()
    }
}

unsafe impl Send for Overlapped {}
unsafe impl Sync for Overlapped {}
