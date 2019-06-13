use crate::event_imp::{Event, Evented, Interests, PollOpt};
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

// Poll is backed by two readiness queues. The first is a system readiness queue
// represented by `sys::Selector`. The system readiness queue handles events
// provided by the system, such as TCP and UDP. The second readiness queue is
// implemented in user space by `ReadinessQueue`. It provides a way to implement
// purely user space `Evented` types.
//
// `ReadinessQueue` is backed by a MPSC queue that supports reuse of linked
// list nodes. This significantly reduces the number of required allocations.
// Each `Registration` / `SetReadiness` pair allocates a single readiness node
// that is used for the lifetime of the registration.
//
// The readiness node also includes a single atomic variable, `state` that
// tracks most of the state associated with the registration. This includes the
// current readiness, interest, poll options, and internal state. When the node
// state is mutated, it is queued in the MPSC channel. A call to
// `ReadinessQueue::poll` will dequeue and process nodes. The node state can
// still be mutated while it is queued in the channel for processing.
// Intermediate state values do not matter as long as the final state is
// included in the call to `poll`. This is the eventually consistent nature of
// the readiness queue.
//
// The readiness node is ref counted using the `ref_count` field. On creation,
// the ref_count is initialized to 3: one `Registration` handle, one
// `SetReadiness` handle, and one for the readiness queue. Since the readiness queue
// doesn't *always* hold a handle to the node, we don't use the Arc type for
// managing ref counts (this is to avoid constantly incrementing and
// decrementing the ref count when pushing & popping from the queue). When the
// `Registration` handle is dropped, the `dropped` flag is set on the node, then
// the node is pushed into the registration queue. When Poll::poll pops the
// node, it sees the drop flag is set, and decrements it's ref count.
//
// The MPSC queue is a modified version of the intrusive MPSC node based queue
// described by 1024cores [1].
//
// The first modification is that two markers are used instead of a single
// `stub`. The second marker is a `sleep_marker` which is used to signal to
// producers that the consumer is going to sleep. This sleep_marker is only used
// when the queue is empty, implying that the only node in the queue is
// `end_marker`.
//
// The second modification is an `until` argument passed to the dequeue
// function. When `poll` encounters a level-triggered node, the node will be
// immediately pushed back into the queue. In order to avoid an infinite loop,
// `poll` before pushing the node, the pointer is saved off and then passed
// again as the `until` argument. If the next node to pop is `until`, then
// `Dequeue::Empty` is returned.
//
// [1] http://www.1024cores.net/home/lock-free-algorithms/queues/intrusive-mpsc-node-based-queue

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
/// use mio::{Events, Poll, Interests, PollOpt, Token};
/// use mio::net::TcpStream;
///
/// use std::net::{TcpListener, SocketAddr};
///
/// // Bind a server socket to connect to.
/// let addr: SocketAddr = "127.0.0.1:0".parse()?;
/// let server = TcpListener::bind(&addr)?;
///
/// // Construct a new `Poll` handle as well as the `Events` we'll store into
/// let mut poll = Poll::new()?;
/// let registry = poll.registry().clone();
/// let mut events = Events::with_capacity(1024);
///
/// // Connect the stream
/// let stream = TcpStream::connect(&server.local_addr()?)?;
///
/// // Register the stream with `Poll`
/// registry.register(&stream, Token(0), Interests::READABLE | Interests::WRITABLE, PollOpt::edge())?;
///
/// // Wait for the socket to become ready. This has to happens in a loop to
/// // handle spurious wakeups.
/// loop {
///     poll.poll(&mut events, None)?;
///
///     for event in &events {
///         if event.token() == Token(0) && event.readiness().is_writable() {
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
/// # Edge-triggered and level-triggered
///
/// An [`Evented`] registration may request edge-triggered events or
/// level-triggered events. This is done by setting `register`'s
/// [`PollOpt`] argument to either [`edge`] or [`level`].
///
/// The difference between the two can be described as follows. Supposed that
/// this scenario happens:
///
/// 1. A [`TcpStream`] is registered with `Poll`.
/// 2. The socket receives 2kb of data.
/// 3. A call to [`Poll::poll`] returns the token associated with the socket
///    indicating readable readiness.
/// 4. 1kb is read from the socket.
/// 5. Another call to [`Poll::poll`] is made.
///
/// If when the socket was registered, edge triggered events were requested,
/// then the call to [`Poll::poll`] done in step **5** will (probably) hang
/// despite there being another 1kb still present in the socket read buffer. The
/// reason for this is that edge-triggered mode delivers events only when
/// changes occur on the monitored [`Evented`]. So, in step *5* the caller might
/// end up waiting for some data that is already present inside the socket
/// buffer.
///
/// With edge-triggered events, operations **must** be performed on the
/// `Evented` type until [`WouldBlock`] is returned. In other words, after
/// receiving an event indicating readiness for a certain operation, one should
/// assume that [`Poll::poll`] may never return another event for the same token
/// and readiness until the operation returns [`WouldBlock`].
///
/// By contrast, when level-triggered notifications was requested, each call to
/// [`Poll::poll`] will return an event for the socket as long as data remains
/// in the socket buffer. Generally, level-triggered events should be avoided if
/// high performance is a concern.
///
/// Since even with edge-triggered events, multiple events can be generated upon
/// receipt of multiple chunks of data, the caller has the option to set the
/// [`oneshot`] flag. This tells `Poll` to disable the associated [`Evented`]
/// after the event is returned from [`Poll::poll`]. The subsequent calls to
/// [`Poll::poll`] will no longer include events for [`Evented`] handles that
/// are disabled even if the readiness state changes. The handle can be
/// re-enabled by calling [`reregister`]. When handles are disabled, internal
/// resources used to monitor the handle are maintained until the handle is
/// dropped or deregistered. This makes re-registering the handle a fast
/// operation.
///
/// For example, in the following scenario:
///
/// 1. A [`TcpStream`] is registered with `Poll`.
/// 2. The socket receives 2kb of data.
/// 3. A call to [`Poll::poll`] returns the token associated with the socket
///    indicating readable readiness.
/// 4. 2kb is read from the socket.
/// 5. Another call to read is issued and [`WouldBlock`] is returned
/// 6. The socket receives another 2kb of data.
/// 7. Another call to [`Poll::poll`] is made.
///
/// Assuming the socket was registered with the [`edge`] and [`oneshot`]
/// options, then the call to [`Poll::poll`] in step 7 would block. This is
/// because, [`oneshot`] tells `Poll` to disable events for the socket after
/// returning an event.
///
/// In order to receive the event for the data received in step 6, the socket
/// would need to be reregistered using [`reregister`].
///
/// [`PollOpt`]: struct.PollOpt.html
/// [`edge`]: struct.PollOpt.html#method.edge
/// [`level`]: struct.PollOpt.html#method.level
/// [`Poll::poll`]: struct.Poll.html#method.poll
/// [`WouldBlock`]: https://doc.rust-lang.org/std/io/enum.ErrorKind.html#variant.WouldBlock
/// [`Evented`]: event/trait.Evented.html
/// [`TcpStream`]: tcp/struct.TcpStream.html
/// [`reregister`]: #method.reregister
/// [`oneshot`]: struct.PollOpt.html#method.oneshot
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
/// that, just because a readiness notification was received, that the
/// associated operation will succeed as well.
///
/// If operation fails with [`WouldBlock`], then the caller should not treat
/// this as an error, but instead should wait until another readiness event is
/// received.
///
/// ### Draining readiness
///
/// When using edge-triggered mode, once a readiness event is received, the
/// corresponding operation must be performed repeatedly until it returns
/// [`WouldBlock`]. Unless this is done, there is no guarantee that another
/// readiness event will be delivered, even if further data is received for the
/// [`Evented`] handle.
///
/// For example, in the first scenario described above, after step 5, even if
/// the socket receives more data there is no guarantee that another readiness
/// event will be delivered.
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
/// [`readable`]: Ready::READABLE
/// [`writable`]: Ready::WRITABLE
/// [`error`]: Ready::ERROR
/// [`hup`]: Ready::HUP
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
/// use mio::{Poll, Interests, PollOpt, Token};
/// use mio::net::TcpStream;
/// use std::time::Duration;
/// use std::thread;
///
/// let sock = TcpStream::connect(&"216.58.193.100:80".parse()?)?;
///
/// thread::sleep(Duration::from_secs(1));
///
/// let mut poll = Poll::new()?;
/// let registry = poll.registry().clone();
///
/// // The connect is not guaranteed to have started until it is registered at
/// // this point
/// registry.register(&sock, Token(0), Interests::READABLE | Interests::WRITABLE, PollOpt::edge())?;
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
/// |      OS    |  Selector |
/// |------------|-----------|
/// | Linux      | [epoll]   |
/// | OS X, iOS  | [kqueue]  |
/// | Windows    | [IOCP]    |
/// | FreeBSD    | [kqueue]  |
/// | Android    | [epoll]   |
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

const WAKE: Token = Token(usize::MAX);

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
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::{Events, Poll, Interests, PollOpt, Token};
    /// use mio::net::TcpStream;
    ///
    /// use std::net::{TcpListener, SocketAddr};
    /// use std::thread;
    ///
    /// // Bind a server socket to connect to.
    /// let addr: SocketAddr = "127.0.0.1:0".parse()?;
    /// let server = TcpListener::bind(&addr)?;
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
    /// let stream = TcpStream::connect(&addr)?;
    ///
    /// // Register the stream with `Poll`
    /// registry.register(
    ///     &stream,
    ///     Token(0),
    ///     Interests::READABLE | Interests::WRITABLE,
    ///     PollOpt::edge())?;
    ///
    /// // Wait for the socket to become ready. This has to happens in a loop to
    /// // handle spurious wakeups.
    /// loop {
    ///     poll.poll(&mut events, None)?;
    ///
    ///     for event in &events {
    ///         if event.token() == Token(0) && event.readiness().is_writable() {
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
            let res = selector.select(&mut events.inner, WAKE, timeout);

            match res {
                Ok(_) => break,
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
        Ok(events.inner.len())
    }
}

fn validate_args(token: Token) -> io::Result<()> {
    if token == WAKE {
        return Err(io::Error::new(io::ErrorKind::Other, "invalid token"));
    }

    Ok(())
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
    /// `interest: Ready`: Specifies which operations `Poll` should monitor for
    /// readiness. `Poll` will only return readiness events for operations
    /// specified by this argument.
    ///
    /// If a socket is registered with readable interest and the socket becomes
    /// writable, no event will be returned from [`poll`].
    ///
    /// The readiness interest for an `Evented` handle can be changed at any
    /// time by calling [`reregister`].
    ///
    /// `opts: PollOpt`: Specifies the registration options. The most common
    /// options being [`level`] for level-triggered events, [`edge`] for
    /// edge-triggered events, and [`oneshot`].
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
    /// [`level`]: struct.PollOpt.html#method.level
    /// [`edge`]: struct.PollOpt.html#method.edge
    /// [`oneshot`]: struct.PollOpt.html#method.oneshot
    /// [`Token`]: struct.Token.html
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::{Events, Poll, Interests, PollOpt, Token};
    /// use mio::net::TcpStream;
    /// use std::time::{Duration, Instant};
    ///
    /// let mut poll = Poll::new()?;
    /// let registry = poll.registry().clone();
    /// let socket = TcpStream::connect(&"216.58.193.100:80".parse()?)?;
    ///
    /// // Register the socket with `poll`
    /// registry.register(
    ///     &socket,
    ///     Token(0),
    ///     Interests::READABLE | Interests::WRITABLE,
    ///     PollOpt::edge())?;
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
        opts: PollOpt,
    ) -> io::Result<()>
    where
        E: Evented,
    {
        validate_args(token)?;

        /*
         * Undefined behavior:
         * - Reusing a token with a different `Evented` without deregistering
         * (or closing) the original `Evented`.
         */
        trace!("registering with poller");

        // Register interests for this socket
        handle.register(self, token, interests, opts)?;

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
    /// use mio::{Poll, Interests, PollOpt, Token};
    /// use mio::net::TcpStream;
    ///
    /// let mut poll = Poll::new()?;
    /// let registry = poll.registry().clone();
    /// let socket = TcpStream::connect(&"216.58.193.100:80".parse()?)?;
    ///
    /// // Register the socket with `poll`, requesting readable
    /// registry.register(
    ///     &socket,
    ///     Token(0),
    ///     Interests::READABLE,
    ///     PollOpt::edge())?;
    ///
    /// // Reregister the socket specifying a different token and write interest
    /// // instead. `PollOpt::edge()` must be specified even though that value
    /// // is not being changed.
    /// registry.reregister(
    ///     &socket,
    ///     Token(2),
    ///     Interests::WRITABLE,
    ///     PollOpt::edge())?;
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
    /// [`readable`]: Ready::READABLE
    /// [`writable`]: Ready::WRITABLE
    pub fn reregister<E: ?Sized>(
        &self,
        handle: &E,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()>
    where
        E: Evented,
    {
        validate_args(token)?;

        trace!("registering with poller");

        // Register interests for this socket
        handle.reregister(self, token, interests, opts)?;

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
    /// use mio::{Events, Poll, Interests, PollOpt, Token};
    /// use mio::net::TcpStream;
    /// use std::time::Duration;
    ///
    /// let mut poll = Poll::new()?;
    /// let registry = poll.registry().clone();
    /// let socket = TcpStream::connect(&"216.58.193.100:80".parse()?)?;
    ///
    /// // Register the socket with `poll`
    /// registry.register(
    ///     &socket,
    ///     Token(0),
    ///     Interests::READABLE,
    ///     PollOpt::edge())?;
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

        // Deregister interests for this socket
        handle.deregister(self)?;

        Ok(())
    }
}

/// A collection of readiness events.
///
/// `Events` is passed as an argument to [`Poll::poll`] and will be used to
/// receive any new readiness events received since the last poll. Usually, a
/// single `Events` instance is created at the same time as a [`Poll`] and
/// reused on each call to [`Poll::poll`].
///
/// See [`Poll`] for more documentation on polling.
///
/// # Examples
///
/// ```
/// # use std::error::Error;
/// # fn try_main() -> Result<(), Box<Error>> {
/// use mio::{Events, Poll};
/// use std::time::Duration;
///
/// let mut events = Events::with_capacity(1024);
/// let mut poll = Poll::new()?;
///
/// assert_eq!(0, events.iter().count());
///
/// // Register `Evented` handles with `poll`
///
/// poll.poll(&mut events, Some(Duration::from_millis(100)))?;
///
/// for event in &events {
///     println!("event={:?}", event);
/// }
/// #     Ok(())
/// # }
/// #
/// # fn main() {
/// #     try_main().unwrap();
/// # }
/// ```
///
/// [`Poll::poll`]: struct.Poll.html#method.poll
/// [`Poll`]: struct.Poll.html
pub struct Events {
    inner: sys::Events,
}

/// [`Events`] iterator.
///
/// This struct is created by the [`iter`] method on [`Events`].
///
/// # Examples
///
/// ```
/// # use std::error::Error;
/// # fn try_main() -> Result<(), Box<Error>> {
/// use mio::{Events, Poll};
/// use std::time::Duration;
///
/// let mut events = Events::with_capacity(1024);
/// let mut poll = Poll::new()?;
///
/// // Register handles with `poll`
///
/// poll.poll(&mut events, Some(Duration::from_millis(100)))?;
///
/// for event in events.iter() {
///     println!("event={:?}", event);
/// }
/// #     Ok(())
/// # }
/// #
/// # fn main() {
/// #     try_main().unwrap();
/// # }
/// ```
///
/// [`Events`]: struct.Events.html
/// [`iter`]: struct.Events.html#method.iter
#[derive(Debug, Clone)]
pub struct Iter<'a> {
    inner: &'a Events,
    pos: usize,
}

/// Owned [`Events`] iterator.
///
/// This struct is created by the `into_iter` method on [`Events`].
///
/// # Examples
///
/// ```
/// # use std::error::Error;
/// # fn try_main() -> Result<(), Box<Error>> {
/// use mio::{Events, Poll};
/// use std::time::Duration;
///
/// let mut events = Events::with_capacity(1024);
/// let mut poll = Poll::new()?;
///
/// // Register handles with `poll`
///
/// poll.poll(&mut events, Some(Duration::from_millis(100)))?;
///
/// for event in events {
///     println!("event={:?}", event);
/// }
/// #     Ok(())
/// # }
/// #
/// # fn main() {
/// #     try_main().unwrap();
/// # }
/// ```
/// [`Events`]: struct.Events.html
#[derive(Debug)]
pub struct IntoIter {
    inner: Events,
    pos: usize,
}

impl Events {
    /// Return a new `Events` capable of holding up to `capacity` events.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Events;
    ///
    /// let events = Events::with_capacity(1024);
    ///
    /// assert_eq!(1024, events.capacity());
    /// ```
    pub fn with_capacity(capacity: usize) -> Events {
        Events {
            inner: sys::Events::with_capacity(capacity),
        }
    }

    /// Returns the number of `Event` values that `self` can hold.
    ///
    /// ```
    /// use mio::Events;
    ///
    /// let events = Events::with_capacity(1024);
    ///
    /// assert_eq!(1024, events.capacity());
    /// ```
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Returns `true` if `self` contains no `Event` values.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Events;
    ///
    /// let events = Events::with_capacity(1024);
    ///
    /// assert!(events.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns an iterator over the `Event` values.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::{Events, Poll};
    /// use std::time::Duration;
    ///
    /// let mut events = Events::with_capacity(1024);
    /// let mut poll = Poll::new()?;
    ///
    /// // Register handles with `poll`
    ///
    /// poll.poll(&mut events, Some(Duration::from_millis(100)))?;
    ///
    /// for event in events.iter() {
    ///     println!("event={:?}", event);
    /// }
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            inner: self,
            pos: 0,
        }
    }

    /// Clearing all `Event` values from container explicitly.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::error::Error;
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::{Events, Poll};
    /// use std::time::Duration;
    ///
    /// let mut events = Events::with_capacity(1024);
    /// let mut poll = Poll::new()?;
    ///
    /// // Register handles with `poll`
    /// for _ in 0..2 {
    ///     events.clear();
    ///     poll.poll(&mut events, Some(Duration::from_millis(100)))?;
    ///
    ///     for event in events.iter() {
    ///         println!("event={:?}", event);
    ///     }
    /// }
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<'a> IntoIterator for &'a Events {
    type Item = Event;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        let ret = self.inner.inner.get(self.pos);
        self.pos += 1;
        ret
    }
}

impl IntoIterator for Events {
    type Item = Event;
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            inner: self,
            pos: 0,
        }
    }
}

impl Iterator for IntoIter {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        let ret = self.inner.inner.get(self.pos);
        self.pos += 1;
        ret
    }
}

impl fmt::Debug for Events {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Events")
            .field("capacity", &self.capacity())
            .finish()
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
