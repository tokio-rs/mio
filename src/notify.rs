use {sys, Evented, Interest, PollOpt, Selector, Token};
use util::BoundedQueue;
use std::{fmt, cmp, io};
use std::sync::Arc;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::Relaxed;

const SLEEP: isize = -1;

/// Send notifications to the event loop, waking it up if necessary. If the
/// event loop is not currently sleeping, avoid using an OS wake-up strategy
/// (eventfd, pipe, ...). Backed by a pre-allocated lock free MPMC queue.
///
/// TODO: Use more efficient wake-up strategy if available
pub struct Notify<M: Send> {
    inner: Arc<NotifyInner<M>>
}

impl<M: Send> Notify<M> {
    #[inline]
    pub fn with_capacity(capacity: usize) -> io::Result<Notify<M>> {
        Ok(Notify {
            inner: Arc::new(try!(NotifyInner::with_capacity(capacity)))
        })
    }

    #[inline]
    pub fn check(&self, max: usize, will_sleep: bool) -> usize {
        self.inner.check(max, will_sleep)
    }

    #[inline]
    pub fn notify(&self, value: M) -> Result<(), NotifyError<M>> {
        self.inner.notify(value)
    }

    #[inline]
    pub fn poll(&self) -> Option<M> {
        self.inner.poll()
    }

    #[inline]
    pub fn cleanup(&self) {
        self.inner.cleanup();
    }
}

impl<M: Send> Clone for Notify<M> {
    fn clone(&self) -> Notify<M> {
        Notify {
            inner: self.inner.clone()
        }
    }
}

impl<M> fmt::Debug for Notify<M> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Notify<?>")
    }
}

unsafe impl<M: Send> Sync for Notify<M> { }
unsafe impl<M: Send> Send for Notify<M> { }

struct NotifyInner<M> {
    state: AtomicIsize,
    queue: BoundedQueue<M>,
    awaken: sys::Awakener
}

impl<M: Send> NotifyInner<M> {
    fn with_capacity(capacity: usize) -> io::Result<NotifyInner<M>> {
        Ok(NotifyInner {
            state: AtomicIsize::new(0),
            queue: BoundedQueue::with_capacity(capacity),
            awaken: try!(sys::Awakener::new())
        })
    }

    fn check(&self, max: usize, will_sleep: bool) -> usize {
        let max = max as isize;
        let mut cur = self.state.load(Relaxed);
        let mut nxt;
        let mut val;

        loop {
            // If there are pending messages, then whether or not the event loop
            // was planning to sleep does not matter - it will not sleep.
            if cur > 0 {
                if max >= cur {
                    nxt = 0;
                } else {
                    nxt = cur - max;
                }
            } else {
                if will_sleep {
                    nxt = SLEEP;
                } else {
                    nxt = 0;
                }
            }

            val = self.state.compare_and_swap(cur, nxt, Relaxed);

            if val == cur {
                break;
            }

            cur = val;
        }

        if cur < 0 {
            0
        } else {
            cmp::min(cur, max) as usize
        }
    }

    fn poll(&self) -> Option<M> {
        self.queue.pop()
    }

    fn notify(&self, value: M) -> Result<(), NotifyError<M>> {
        // First, push the message onto the queue
        if let Err(value) = self.queue.push(value) {
            return Err(NotifyError::Full(value));
        }

        let mut cur = self.state.load(Relaxed);
        let mut nxt;
        let mut val;

        loop {
            nxt = if cur == SLEEP { 1 } else { cur + 1 };
            val = self.state.compare_and_swap(cur, nxt, Relaxed);

            if val == cur {
                break;
            }

            cur = val;
        }

        if cur == SLEEP {
            if let Err(e) = self.awaken.wakeup() {
                return Err(NotifyError::Io(e));
            }
        }

        Ok(())
    }

    fn cleanup(&self) {
        self.awaken.cleanup();
    }
}

impl<M: Send> Evented for Notify<M> {
    fn register(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.inner.awaken.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: Interest, opts: PollOpt) -> io::Result<()> {
        self.inner.awaken.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.inner.awaken.deregister(selector)
    }
}

pub enum NotifyError<T> {
    Io(io::Error),
    Full(T),
}

impl<M> fmt::Debug for NotifyError<M> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            NotifyError::Io(ref e) => {
                write!(fmt, "NotifyError::Io({:?})", e)
            }
            NotifyError::Full(..) => {
                write!(fmt, "NotifyError::Full(..)")
            }
        }
    }
}
