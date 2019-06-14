use std::io;

use crate::{poll, sys, Registry, Token};

/// Waker allows cross-thread waking of [`Poll`].
///
/// When created it will cause events with [`Ready::READABLE`] and the provided
/// `token` if [`wake`] is called, possibly from another thread.
///
/// [`Poll`]: crate::Poll
///
/// # Notes
///
/// `Waker` events are only guaranteed to be delivered while the `Waker` value
/// is alive.
///
/// Only a single `Waker` should active per [`Poll`], if multiple threads need
/// access to the `Waker` it can be shared via for example an `Arc`. What
/// happens if multiple `Waker`s are registered with the same `Poll` is
/// undefined.
///
/// [`Ready::READABLE`]: crate::Ready::READABLE
/// [`wake`]: Waker::wake
///
/// # Implementation notes
///
/// On platforms that support kqueue this will use the `EVFILT_USER` event
/// filter, see [implementation notes of `Poll`] to see what platform supports
/// kqueue. On Linux it uses [eventfd].
///
/// [implementation notes of `Poll`]: ../index.html#implementation-notes
/// [eventfd]: http://man7.org/linux/man-pages/man2/eventfd.2.html
///
/// # Examples
///
/// Wake an [`Poll`] from another thread.
///
/// ```ignore
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use std::io;
/// use std::thread;
/// use std::time::Duration;
/// use std::sync::Arc;
///
/// use mio::event::Event;
/// use mio::{Events, Ready, Token, Poll, Waker};
///
/// const WAKE_TOKEN: Token = Token(10);
///
/// let mut poll = Poll::new()?;
/// let mut events = Events::with_capacity(2);
///
/// let waker = Arc::new(Waker::new(poll.registry(), WAKE_TOKEN)?);
///
/// // We need to keep the Waker alive, so we'll create a clone for the
/// // thread we create below.
/// let waker1 = waker.clone();
/// let handle = thread::spawn(move || {
///     // Working hard, or hardly working?
///     thread::sleep(Duration::from_millis(500));
///
///     // Now we'll wake the queue on the other thread.
///     waker1.wake().expect("unable to wake");
/// });
///
/// // On our current thread we'll poll for events, without a timeout.
/// poll.poll(&mut events, None)?;
///
/// // After about 500 milliseconds we should we awoken by the other thread we
/// // started, getting a single event.
/// assert!(!events.is_empty());
/// for event in &events {
///     assert_eq!(event, Event::new(Ready::READABLE, WAKE_TOKEN));
/// }
/// # handle.join().unwrap();
/// #     Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Waker {
    inner: sys::Waker,
}

impl Waker {
    /// Create a new `Waker`.
    pub fn new(registry: &Registry, token: Token) -> io::Result<Waker> {
        sys::Waker::new(poll::selector(&registry), token).map(|inner| Waker { inner })
    }

    /// Wake up the [`Poll`] associated with this `Waker`.
    ///
    /// [`Poll`]: crate::Poll
    pub fn wake(&self) -> io::Result<()> {
        self.inner.wake()
    }
}
