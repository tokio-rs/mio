use std::sync::Arc;
use std::sync::atomic::{ AtomicBool, Ordering };
use std::sync::mpsc::{SyncSender,
                      Receiver,
                      SendError,
                      TryRecvError,
                      sync_channel};
use error::MioResult;
use io::IoHandle;
use os;

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
    pub fn with_capacity(capacity: usize) -> MioResult<Notify<M>> {
        Ok(Notify {
            inner: Arc::new(try!(NotifyInner::with_capacity(capacity)))
        })
    }

    #[inline]
    pub fn notify(&self, value: M) -> Result<(), SendError<M>> {
        self.inner.notify(value)
    }

    #[inline]
    pub fn poll(&self) -> Result<M, TryRecvError> {
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

struct NotifyInner<M> {
    state: AtomicBool,
    queue: Receiver<M>,
    queue_tx: SyncSender<M>,
    //queue: BoundedQueue<M>,
    awaken: os::Awakener
}

unsafe impl<M> Sync for NotifyInner<M> {}

impl<M: Send> NotifyInner<M> {
    fn with_capacity(capacity: usize) -> MioResult<NotifyInner<M>> {
        let (tx, rx) = sync_channel(capacity);
        Ok(NotifyInner {
            state: AtomicBool::new(false),
            queue: rx, //BoundedQueue::with_capacity(capacity),
            queue_tx: tx,
            awaken: try!(os::Awakener::new())
        })
    }

    fn poll(&self) -> Result<M, TryRecvError> {
        //self.queue.pop()
        let res = self.queue.try_recv();
        if res.is_err() {
            self.state.store(false, Ordering::Relaxed);
        }
        res
    }

    fn notify(&self, value: M) -> Result<(), SendError<M>> {
        // First, push the message onto the queue
        let res = self.queue_tx.send(value);
        if res.is_ok() && !self.state.load(Ordering::Relaxed) {
            self.state.store(true, Ordering::Relaxed);
            self.awaken.wakeup().ok().expect("WAAAGH BEES");
        }
        res
    }

    fn cleanup(&self) {
        self.awaken.cleanup();
        self.state.store(false, Ordering::Relaxed);
    }
}

impl<M: Send> IoHandle for Notify<M> {
    fn desc(&self) -> &os::IoDesc {
        self.inner.awaken.desc()
    }
}
