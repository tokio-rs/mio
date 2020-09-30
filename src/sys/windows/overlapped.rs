use std::cell::UnsafeCell;
use std::fmt;

use winapi::um::minwinbase::OVERLAPPED_ENTRY;
#[cfg(feature = "os-util")]
use winapi::um::minwinbase::OVERLAPPED;

// See sys::windows module docs for why this exists.
//
// The gist of it is that `Selector` assumes that all `OVERLAPPED` pointers are
// actually inside one of these structures so it can use the `Callback` stored
// right after it.
//
// We use repr(C) here to ensure that we can assume the overlapped pointer is
// at the start of the structure so we can just do a cast.
/// A wrapper around an internal instance over `miow::Overlapped` which is in
/// turn a wrapper around the Windows type `OVERLAPPED`.
///
/// This type is required to be used for all IOCP operations on handles that are
/// registered with an event loop. The event loop will receive notifications
/// over `OVERLAPPED` pointers that have completed, and it will cast that
/// pointer to a pointer to this structure and invoke the associated callback.
#[repr(C)]
pub(crate) struct Overlapped {
    inner: UnsafeCell<miow::Overlapped>,
    pub(crate) callback: fn(&OVERLAPPED_ENTRY),
}

#[cfg(feature = "os-util")]
impl Overlapped {
    /// Creates a new `Overlapped` which will invoke the provided `cb` callback
    /// whenever it's triggered.
    ///
    /// The returned `Overlapped` must be used as the `OVERLAPPED` passed to all
    /// I/O operations that are registered with mio's event loop. When the I/O
    /// operation associated with an `OVERLAPPED` pointer completes the event
    /// loop will invoke the function pointer provided by `cb`.
    pub fn new(cb: fn(&OVERLAPPED_ENTRY)) -> Overlapped {
        Overlapped {
            inner: UnsafeCell::new(miow::Overlapped::zero()),
            callback: cb,
        }
    }

    /// Get the underlying `Overlapped` instance as a raw pointer.
    ///
    /// This can be useful when only a shared borrow is held and the overlapped
    /// pointer needs to be passed down to winapi.
    pub fn as_mut_ptr(&self) -> *mut OVERLAPPED {
        unsafe { (*self.inner.get()).raw() }
    }
}

impl fmt::Debug for Overlapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Overlapped").finish()
    }
}

// Overlapped's APIs are marked as unsafe Overlapped's APIs are marked as
// unsafe as they must be used with caution to ensure thread safety. The
// structure itself is safe to send across threads.
unsafe impl Send for Overlapped {}
unsafe impl Sync for Overlapped {}
