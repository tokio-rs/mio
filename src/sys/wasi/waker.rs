use crate::sys::wasi::{io_err, Selector};
use crate::{Interest, Token};

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

// A pair of connected FDs to emulate the "eventfd".
static FD_READ: AtomicU32 = AtomicU32::new(0);
static FD_WRITE: AtomicU32 = AtomicU32::new(0);

// A fast path implementation for single thread only use case. Instead
// of waking up the poll, if enabled, a set of global atomic variables
// can be used to check the wake up event before actually getting into
// the poll.
static FAST_WAKE: AtomicBool = AtomicBool::new(false);
static FAST_WAKE_AWAKE: AtomicBool = AtomicBool::new(false);
static FAST_WAKE_COUNT: AtomicUsize = AtomicUsize::new(0);
const FAST_WAKE_MAX: usize = 32;

/// Waker backed by two connected fds.
///
/// Waker controls both the sending and receiving ends and empties the pipe
/// if writing to it (waking) fails.
#[derive(Debug, Default)]
pub struct Waker {
    inited: bool,
    fast_wake: bool,
    sender: wasi::Fd,
    receiver: wasi::Fd,
}

pub(crate) fn fast_wake_awake() -> bool {
    // If waker has not been initialized, this will always return
    // false.
    if !FAST_WAKE_AWAKE.swap(false, Ordering::SeqCst) {
        return false;
    }

    // To keep fairness for other event sources, force a poll after
    // several fast wakeups.
    let cnt = FAST_WAKE_COUNT.load(Ordering::SeqCst);
    if cnt > FAST_WAKE_MAX {
        FAST_WAKE_COUNT.store(0, Ordering::SeqCst);
        return false;
    } else {
        FAST_WAKE_COUNT.store(cnt + 1, Ordering::SeqCst);
        return true;
    }
}

pub(crate) fn init_waker(receiver: u32, sender: u32, single_threaded: bool) {
    FD_READ.store(receiver, Ordering::SeqCst);
    FD_WRITE.store(sender, Ordering::SeqCst);
    if single_threaded {
        FAST_WAKE.store(true, Ordering::SeqCst);
    }
}

impl Waker {
    pub(crate) fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        let (receiver, sender, fast_wake) = (
            FD_READ.load(Ordering::SeqCst),
            FD_WRITE.load(Ordering::SeqCst),
            FAST_WAKE.load(Ordering::SeqCst),
        );

        if receiver == sender {
            return Ok(Waker {
                inited: false,
                ..Default::default()
            });
        }

        // Make sure these FDs are non-blocking to deal with buffer
        // full on event write.
        unsafe { wasi::fd_fdstat_set_flags(sender, wasi::FDFLAGS_NONBLOCK) }.map_err(io_err)?;
        unsafe { wasi::fd_fdstat_set_flags(receiver, wasi::FDFLAGS_NONBLOCK) }.map_err(io_err)?;

        selector.register(receiver, token, Interest::READABLE)?;
        Ok(Waker {
            inited: true,
            fast_wake,
            sender,
            receiver,
        })
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        if !self.inited {
            return Ok(());
        }

        if self.fast_wake {
            let already_awake = FAST_WAKE_AWAKE.swap(true, Ordering::SeqCst);
            if already_awake {
                return Ok(());
            }
        }

        let buf = [1];
        let iov = wasi::Ciovec {
            buf: buf.as_ptr(),
            buf_len: buf.len(),
        };

        match unsafe { wasi::fd_write(self.sender, &[iov]) } {
            Ok(_) => Ok(()),
            Err(err) if err == wasi::ERRNO_AGAIN => {
                // The reading end is full so we'll empty the buffer and try
                // again.
                self.empty();
                self.wake()
            }
            Err(err) if err == wasi::ERRNO_INTR => self.wake(),
            Err(err) => Err(io_err(err)),
        }
    }

    /// Empty the pipe's buffer, only need to call this if `wake` fails.
    /// This ignores any errors.
    fn empty(&self) {
        let mut buf = [0; 4096];
        let iov = wasi::Iovec {
            buf: buf.as_mut_ptr(),
            buf_len: buf.len(),
        };
        loop {
            match unsafe { wasi::fd_read(self.receiver, &[iov]) } {
                Ok(n) if n > 0 => continue,
                _ => return,
            }
        }
    }
}
