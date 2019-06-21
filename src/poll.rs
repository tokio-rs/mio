use crate::event::{Evented, Events};
use crate::Interests;
use crate::{sys, Token};
use log::trace;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(unix)]
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{fmt, io, usize};

/// Polls for readiness events on all registered values.
///
/// `Poll` allows a program to monitor a large number of `Evented` types,
/// waiting until one or more become "ready" for some class of operations; e.g.
/// reading and writing. An `Evented` type is considered ready if it is possible
/// to immediately perform a corresponding operation; e.g. [`read`] or
/// [`write`].
///
/// To use `Poll`, an `Evented` type must first be registered with the `Poll`
/// instance using the [`register`] method on its associated `Register`,
/// supplying readiness interest. The readiness interest tells `Poll` which
/// specific operations on the handle to monitor for readiness. A `Token` is
/// also passed to the [`register`] function. When `Poll` returns a readiness
/// event, it will include this token.  This associates the event with the
/// `Evented` handle that generated the event.
///
/// [`read`]: tcp/struct.TcpStream.html#method.read
/// [`write`]: tcp/struct.TcpStream.html#method.write
/// [`register`]: #method.register
///
/// # Examples
///
/// A basic example -- establishing a `TcpStream` connection.
///
/// ```
/// # use std::error::Error;
/// # fn try_main() -> Result<(), Box<Error>> {
/// use mio::{Events, Poll, Interests, Token};
/// use mio::net::TcpStream;
///
/// use std::net::{TcpListener, SocketAddr};
///
/// // Bind a server socket to connect to.
/// let addr: SocketAddr = "127.0.0.1:0".parse()?;
/// let server = TcpListener::bind(addr)?;
///
/// // Construct a new `Poll` handle as well as the `Events` we'll store into
/// let mut poll = Poll::new()?;
/// let registry = poll.registry().clone();
/// let mut events = Events::with_capacity(1024);
///
/// // Connect the stream
/// let stream = TcpStream::connect(server.local_addr()?)?;
///
/// // Register the stream with `Poll`
/// registry.register(&stream, Token(0), Interests::READABLE | Interests::WRITABLE)?;
///
/// // Wait for the socket to become ready. This has to happens in a loop to
/// // handle spurious wakeups.
/// loop {
///     poll.poll(&mut events, None)?;
///
///     for event in &events {
///         if event.token() == Token(0) && event.is_writable() {
///             // The socket connected (probably, it could still be a spurious
///             // wakeup)
///             return Ok(());
///         }
///     }
/// }
/// #     Ok(())
/// # }
/// #
/// # fn main() {
/// #     try_main().unwrap();
/// # }
/// ```
///
/// # Portability
///
/// Using `Poll` provides a portable interface across supported platforms as
/// long as the caller takes the following into consideration:
///
/// ### Spurious events
///
/// [`Poll::poll`] may return readiness events even if the associated
/// [`Evented`] handle is not actually ready. Given the same code, this may
/// happen more on some platforms than others. It is important to never assume
/// that, just because a readiness event was received, that the associated
/// operation will succeed as well.
///
/// If operation fails with [`WouldBlock`], then the caller should not treat
/// this as an error, but instead should wait until another readiness event is
/// received.
///
/// ### Draining readiness
///
/// Once a readiness event is received, the corresponding operation must be
/// performed repeatedly until it returns [`WouldBlock`]. Unless this is done,
/// there is no guarantee that another readiness event will be delivered, even
/// if further data is received for the [`Evented`] handle.
///
/// [`WouldBlock`]: std::io::ErrorKind::WouldBlock
///
/// ### Readiness operations
///
/// The only readiness operations that are guaranteed to be present on all
/// supported platforms are [`readable`] and [`writable`]. All other readiness
/// operations may have false negatives and as such should be considered
/// **hints**. This means that if a socket is registered with [`readable`],
/// [`error`], and [`hup`] interest, and either an error or hup is received, a
/// readiness event will be generated for the socket, but it **may** only
/// include `readable` readiness. Also note that, given the potential for
/// spurious events, receiving a readiness event with `hup` or `error` doesn't
/// actually mean that a `read` on the socket will return a result matching the
/// readiness event.
///
/// In other words, portable programs that explicitly check for [`hup`] or
/// [`error`] readiness should be doing so as an **optimization** and always be
/// able to handle an error or HUP situation when performing the actual read
/// operation.
///
/// [`readable`]: crate::event::Event::is_readable
/// [`writable`]: crate::event::Event::is_writable
/// [`error`]: crate::event::Event::is_error
/// [`hup`]: crate::event::Event::is_hup
///
/// ### Registering handles
///
/// Unless otherwise noted, it should be assumed that types implementing
/// [`Evented`] will never become ready unless they are registered with `Poll`.
///
/// For example:
///
/// ```
/// # use std::error::Error;
/// # fn try_main() -> Result<(), Box<Error>> {
/// use mio::{Poll, Interests, Token};
/// use mio::net::TcpStream;
/// use std::time::Duration;
/// use std::thread;
///
/// let sock = TcpStream::connect("216.58.193.100:80".parse()?)?;
///
/// thread::sleep(Duration::from_secs(1));
///
/// let mut poll = Poll::new()?;
/// let registry = poll.registry().clone();
///
/// // The connect is not guaranteed to have started until it is registered at
/// // this point
/// registry.register(&sock, Token(0), Interests::READABLE | Interests::WRITABLE)?;
/// #     Ok(())
/// # }
/// #
/// # fn main() {
/// #     try_main().unwrap();
/// # }
/// ```
///
/// # Implementation notes
///
/// `Poll` is backed by the selector provided by the operating system.
///
/// |      OS       |  Selector |
/// |---------------|-----------|
/// | Android       | [epoll]   |
/// | Bitrig        | [kqueue]  |
/// | DragonFly BSD | [kqueue]  |
/// | FreeBSD       | [kqueue]  |
/// | Linux         | [epoll]   |
/// | NetBSD        | [kqueue]  |
/// | OpenBSD       | [kqueue]  |
/// | Solaris       | [epoll]   |
/// | Windows       | [IOCP]    |
/// | iOS           | [kqueue]  |
/// | macOS         | [kqueue]  |
///
/// On all supported platforms, socket operations are handled by using the
/// system selector. Platform specific extensions (e.g. [`EventedFd`]) allow
/// accessing other features provided by individual system selectors. For
/// example, Linux's [`signalfd`] feature can be used by registering the FD with
/// `Poll` via [`EventedFd`].
///
/// On all platforms except windows, a call to [`Poll::poll`] is mostly just a
/// direct call to the system selector. However, [IOCP] uses a completion model
/// instead of a readiness model. In this case, `Poll` must adapt the completion
/// model Mio's API. While non-trivial, the bridge layer is still quite
/// efficient. The most expensive part being calls to `read` and `write` require
/// data to be copied into an intermediate buffer before it is passed to the
/// kernel.
///
/// Notifications generated by [`SetReadiness`] are handled by an internal
/// readiness queue. A single call to [`Poll::poll`] will collect events from
/// both from the system selector and the internal readiness queue.
///
/// [epoll]: http://man7.org/linux/man-pages/man7/epoll.7.html
/// [kqueue]: https://www.freebsd.org/cgi/man.cgi?query=kqueue&sektion=2
/// [IOCP]: https://msdn.microsoft.com/en-us/library/windows/desktop/aa365198(v=vs.85).aspx
/// [`signalfd`]: http://man7.org/linux/man-pages/man2/signalfd.2.html
/// [`EventedFd`]: unix/struct.EventedFd.html
/// [`SetReadiness`]: struct.SetReadiness.html
/// [`Poll::poll`]: struct.Poll.html#method.poll
pub struct Poll {
    registry: Registry,
}

/// Registers I/O resources.
#[derive(Clone)]
pub struct Registry {
    selector: Arc<sys::Selector>,
}

/// Used to associate an IO type with a Selector
#[derive(Debug)]
pub struct SelectorId {
    id: AtomicUsize,
}

/*
 *
 * ===== Poll =====
 *
 */

impl Poll {
    /// Return a new `Poll` handle.
    ///
    /// This function will make a syscall to the operating system to create the
    /// system selector. If this syscall fails, `Poll::new` will return with the
    /// error.
    ///
    /// See [struct] level docs for more details.
    ///
    /// [struct]: struct.Poll.html
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::{Poll, Events};
    /// use std::time::Duration;
    ///
    /// let mut poll = match Poll::new() {
    ///     Ok(poll) => poll,
    ///     Err(e) => panic!("failed to create Poll instance; err={:?}", e),
    /// };
    ///
    /// // Create a structure to receive polled events
    /// let mut events = Events::with_capacity(1024);
    ///
    /// // Wait for events, but none will be received because no `Evented`
    /// // handles have been registered with this `Poll` instance.
    /// let n = poll.poll(&mut events, Some(Duration::from_millis(500)))?;
    /// assert_eq!(n, 0);
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn new() -> io::Result<Poll> {
        is_send::<Poll>();
        is_sync::<Poll>();

        let selector = Arc::new(sys::Selector::new()?);

        let registry = Registry { selector };

        Ok(Poll { registry })
    }

    /// Return a reference to the associated `Registry`.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Wait for readiness events
    ///
    /// Blocks the current thread and waits for readiness events for any of the
    /// `Evented` handles that have been registered with this `Poll` instance.
    /// The function will block until either at least one readiness event has
    /// been received or `timeout` has elapsed. A `timeout` of `None` means that
    /// `poll` will block until a readiness event has been received.
    ///
    /// The supplied `events` will be cleared and newly received readiness events
    /// will be pushed onto the end. At most `events.capacity()` events will be
    /// returned. If there are further pending readiness events, they will be
    /// returned on the next call to `poll`.
    ///
    /// A single call to `poll` may result in multiple readiness events being
    /// returned for a single `Evented` handle. For example, if a TCP socket
    /// becomes both readable and writable, it may be possible for a single
    /// readiness event to be returned with both [`readable`] and [`writable`]
    /// readiness **OR** two separate events may be returned, one with
    /// [`readable`] set and one with [`writable`] set.
    ///
    /// Note that the `timeout` will be rounded up to the system clock
    /// granularity (usually 1ms), and kernel scheduling delays mean that
    /// the blocking interval may be overrun by a small amount.
    ///
    /// `poll` returns the number of readiness events that have been pushed into
    /// `events` or `Err` when an error has been encountered with the system
    /// selector.  The value returned is deprecated and will be removed in 0.7.0.
    /// Accessing the events by index is also deprecated.  Events can be
    /// inserted by other events triggering, thus making sequential access
    /// problematic.  Use the iterator API instead.  See [`iter`].
    ///
    /// See the [struct] level documentation for a higher level discussion of
    /// polling.
    ///
    /// [`readable`]: struct.Interests.html#method.readable
    /// [`writable`]: struct.Interests.html#method.writable
    /// [struct]: #
    /// [`iter`]: struct.Events.html#method.iter
    ///
    /// # Examples
    ///
    /// A basic example -- establishing a `TcpStream` connection.
    ///
    /// ```
    /// # use std::error::Error;
    /// # fn try_main() -> Result<(), Box<dyn Error>> {
    /// use mio::{Events, Poll, Interests, Token};
    /// use mio::net::TcpStream;
    ///
    /// use std::net::{TcpListener, SocketAddr};
    /// use std::thread;
    ///
    /// // Bind a server socket to connect to.
    /// let addr: SocketAddr = "127.0.0.1:0".parse()?;
    /// let server = TcpListener::bind(addr)?;
    /// let addr = server.local_addr()?.clone();
    ///
    /// // Spawn a thread to accept the socket
    /// thread::spawn(move || {
    ///     let _ = server.accept();
    /// });
    ///
    /// // Construct a new `Poll` handle as well as the `Events` we'll store into
    /// let mut poll = Poll::new()?;
    /// let registry = poll.registry().clone();
    /// let mut events = Events::with_capacity(1024);
    ///
    /// // Connect the stream
    /// let stream = TcpStream::connect(addr)?;
    ///
    /// // Register the stream with `Poll`
    /// registry.register(
    ///     &stream,
    ///     Token(0),
    ///     Interests::READABLE | Interests::WRITABLE)?;
    ///
    /// // Wait for the socket to become ready. This has to happens in a loop to
    /// // handle spurious wakeups.
    /// loop {
    ///     poll.poll(&mut events, None)?;
    ///
    ///     for event in &events {
    ///         if event.token() == Token(0) && event.is_writable() {
    ///             // The socket connected (probably, it could still be a spurious
    ///             // wakeup)
    ///             return Ok(());
    ///         }
    ///     }
    /// }
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    ///
    /// [struct]: #
    pub fn poll(&mut self, events: &mut Events, timeout: Option<Duration>) -> io::Result<usize> {
        self.poll2(events, timeout, false)
    }

    /// Like `poll`, but may be interrupted by a signal
    ///
    /// If `poll` is inturrupted while blocking, it will transparently retry the syscall.  If you
    /// want to handle signals yourself, however, use `poll_interruptible`.
    pub fn poll_interruptible(
        &mut self,
        events: &mut Events,
        timeout: Option<Duration>,
    ) -> io::Result<usize> {
        self.poll2(events, timeout, true)
    }

    fn poll2(
        &mut self,
        events: &mut Events,
        mut timeout: Option<Duration>,
        interruptible: bool,
    ) -> io::Result<usize> {
        let selector = &*self.registry.selector;

        loop {
            let now = Instant::now();
            // First get selector events
            let res = selector.select(events.sys(), timeout);

            match res {
                Ok(()) => break,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted && !interruptible => {
                    // Interrupted by a signal; update timeout if necessary and retry
                    if let Some(to) = timeout {
                        let elapsed = now.elapsed();
                        if elapsed >= to {
                            break;
                        } else {
                            timeout = Some(to - elapsed);
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }

        // Return number of polled events
        Ok(events.sys().len())
    }
}

impl fmt::Debug for Poll {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Poll").finish()
    }
}

impl fmt::Debug for Registry {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Registry").finish()
    }
}

#[cfg(unix)]
impl AsRawFd for Poll {
    fn as_raw_fd(&self) -> RawFd {
        self.registry.selector.as_raw_fd()
    }
}

impl Registry {
    /// Register an `Evented` handle with the `Poll` instance.
    ///
    /// Once registered, the `Poll` instance will monitor the `Evented` handle
    /// for readiness state changes. When it notices a state change, it will
    /// return a readiness event for the handle the next time [`poll`] is
    /// called.
    ///
    /// See the [`struct`] docs for a high level overview.
    ///
    /// # Arguments
    ///
    /// `handle: &E: Evented`: This is the handle that the `Poll` instance
    /// should monitor for readiness state changes.
    ///
    /// `token: Token`: The caller picks a token to associate with the socket.
    /// When [`poll`] returns an event for the handle, this token is included.
    /// This allows the caller to map the event to its handle. The token
    /// associated with the `Evented` handle can be changed at any time by
    /// calling [`reregister`].
    ///
    /// `token` cannot be `Token(usize::MAX)` as it is reserved for internal
    /// usage.
    ///
    /// See documentation on [`Token`] for an example showing how to pick
    /// [`Token`] values.
    ///
    /// `interest: Interests`: Specifies which operations `Poll` should monitor
    /// for readiness. `Poll` will only return readiness events for operations
    /// specified by this argument.
    ///
    /// If a socket is registered with readable interest and the socket becomes
    /// writable, no event will be returned from [`poll`].
    ///
    /// The readiness interest for an `Evented` handle can be changed at any
    /// time by calling [`reregister`].
    ///
    /// The registration options for an `Evented` handle can be changed at any
    /// time by calling [`reregister`].
    ///
    /// # Notes
    ///
    /// Unless otherwise specified, the caller should assume that once an
    /// `Evented` handle is registered with a `Poll` instance, it is bound to
    /// that `Poll` instance for the lifetime of the `Evented` handle. This
    /// remains true even if the `Evented` handle is deregistered from the poll
    /// instance using [`deregister`].
    ///
    /// This function is **thread safe**. It can be called concurrently from
    /// multiple threads.
    ///
    /// [`struct`]: #
    /// [`reregister`]: #method.reregister
    /// [`deregister`]: #method.deregister
    /// [`poll`]: #method.poll
    /// [`Token`]: struct.Token.html
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::{Events, Poll, Interests, Token};
    /// use mio::net::TcpStream;
    /// use std::time::{Duration, Instant};
    ///
    /// let mut poll = Poll::new()?;
    /// let registry = poll.registry().clone();
    /// let socket = TcpStream::connect("216.58.193.100:80".parse()?)?;
    ///
    /// // Register the socket with `poll`
    /// registry.register(
    ///     &socket,
    ///     Token(0),
    ///     Interests::READABLE | Interests::WRITABLE)?;
    ///
    /// let mut events = Events::with_capacity(1024);
    /// let start = Instant::now();
    /// let timeout = Duration::from_millis(500);
    ///
    /// loop {
    ///     let elapsed = start.elapsed();
    ///
    ///     if elapsed >= timeout {
    ///         // Connection timed out
    ///         return Ok(());
    ///     }
    ///
    ///     let remaining = timeout - elapsed;
    ///     poll.poll(&mut events, Some(remaining))?;
    ///
    ///     for event in &events {
    ///         if event.token() == Token(0) {
    ///             // Something (probably) happened on the socket.
    ///             return Ok(());
    ///         }
    ///     }
    /// }
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn register<E: ?Sized>(
        &self,
        handle: &E,
        token: Token,
        interests: Interests,
    ) -> io::Result<()>
    where
        E: Evented,
    {
        trace!("registering with poller");
        handle.register(self, token, interests)?;
        Ok(())
    }

    /// Re-register an `Evented` handle with the `Poll` instance.
    ///
    /// Re-registering an `Evented` handle allows changing the details of the
    /// registration. Specifically, it allows updating the associated `token`,
    /// `interest`, and `opts` specified in previous `register` and `reregister`
    /// calls.
    ///
    /// The `reregister` arguments fully override the previous values. In other
    /// words, if a socket is registered with [`readable`] interest and the call
    /// to `reregister` specifies [`writable`], then read interest is no longer
    /// requested for the handle.
    ///
    /// The `Evented` handle must have previously been registered with this
    /// instance of `Poll` otherwise the call to `reregister` will return with
    /// an error.
    ///
    /// `token` cannot be `Token(usize::MAX)` as it is reserved for internal
    /// usage.
    ///
    /// See the [`register`] documentation for details about the function
    /// arguments and see the [`struct`] docs for a high level overview of
    /// polling.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::{Poll, Interests, Token};
    /// use mio::net::TcpStream;
    ///
    /// let mut poll = Poll::new()?;
    /// let registry = poll.registry().clone();
    /// let socket = TcpStream::connect("216.58.193.100:80".parse()?)?;
    ///
    /// // Register the socket with `poll`, requesting readable
    /// registry.register(
    ///     &socket,
    ///     Token(0),
    ///     Interests::READABLE)?;
    ///
    /// // Reregister the socket specifying write interest instead. Even though
    /// // the token is the same it must be specified.
    /// registry.reregister(
    ///     &socket,
    ///     Token(2),
    ///     Interests::WRITABLE)?;
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    ///
    /// [`struct`]: #
    /// [`register`]: #method.register
    /// [`readable`]: crate::event::Event::is_readable
    /// [`writable`]: crate::event::Event::is_writable
    pub fn reregister<E: ?Sized>(
        &self,
        handle: &E,
        token: Token,
        interests: Interests,
    ) -> io::Result<()>
    where
        E: Evented,
    {
        trace!("registering with poller");
        handle.reregister(self, token, interests)?;
        Ok(())
    }

    /// Deregister an `Evented` handle with the `Poll` instance.
    ///
    /// When an `Evented` handle is deregistered, the `Poll` instance will
    /// no longer monitor it for readiness state changes. Unlike disabling
    /// handles with oneshot, deregistering clears up any internal resources
    /// needed to track the handle.  After an explicit call to this
    /// method completes, it is guaranteed that the token previously
    /// registered to this handle will not be returned by a future
    /// poll, so long as a happens-before relationship is established
    /// between this call and the poll.
    ///
    /// A handle can be passed back to `register` after it has been
    /// deregistered; however, it must be passed back to the **same** `Poll`
    /// instance.
    ///
    /// `Evented` handles are automatically deregistered when they are dropped.
    /// It is common to never need to explicitly call `deregister`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::{Events, Poll, Interests, Token};
    /// use mio::net::TcpStream;
    /// use std::time::Duration;
    ///
    /// let mut poll = Poll::new()?;
    /// let registry = poll.registry().clone();
    /// let socket = TcpStream::connect("216.58.193.100:80".parse()?)?;
    ///
    /// // Register the socket with `poll`
    /// registry.register(
    ///     &socket,
    ///     Token(0),
    ///     Interests::READABLE)?;
    ///
    /// registry.deregister(&socket)?;
    ///
    /// let mut events = Events::with_capacity(1024);
    ///
    /// // Set a timeout because this poll should never receive any events.
    /// let n = poll.poll(&mut events, Some(Duration::from_secs(1)))?;
    /// assert_eq!(0, n);
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn deregister<E: ?Sized>(&self, handle: &E) -> io::Result<()>
    where
        E: Evented,
    {
        trace!("deregistering handle with poller");
        handle.deregister(self)?;
        Ok(())
    }
}

// ===== Accessors for internal usage =====

pub fn selector(registry: &Registry) -> &sys::Selector {
    &registry.selector
}

fn is_send<T: Send>() {}
fn is_sync<T: Sync>() {}

impl SelectorId {
    pub fn new() -> SelectorId {
        SelectorId {
            id: AtomicUsize::new(0),
        }
    }

    pub fn associate_selector(&self, registry: &Registry) -> io::Result<()> {
        let selector_id = self.id.load(Ordering::SeqCst);

        if selector_id != 0 && selector_id != registry.selector.id() {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "socket already registered",
            ))
        } else {
            self.id.store(registry.selector.id(), Ordering::SeqCst);
            Ok(())
        }
    }
}

impl Clone for SelectorId {
    fn clone(&self) -> SelectorId {
        SelectorId {
            id: AtomicUsize::new(self.id.load(Ordering::SeqCst)),
        }
    }
}

#[test]
#[cfg(unix)]
pub fn as_raw_fd() {
    let poll = Poll::new().unwrap();
    assert!(poll.as_raw_fd() > 0);
}
