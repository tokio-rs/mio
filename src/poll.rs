use {sys, Evented, Token};
use event::{self, EventSet, Event, PollOpt};
use std::{fmt, io, mem, ptr, usize};
use std::cell::{UnsafeCell, Cell};
use std::isize;
use std::marker;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicPtr, Ordering};
use std::time::Duration;

const MAX_REFCOUNT: usize = (isize::MAX) as usize;

/// The `Poll` type acts as an interface allowing a program to wait on a set of
/// IO handles until one or more become "ready" to be operated on. An IO handle
/// is considered ready to operate on when the given operation can complete
/// without blocking.
///
/// To use `Poll`, an IO handle must first be registered with the `Poll`
/// instance using the `register()` handle. An `EventSet` representing the
/// program's interest in the socket is specified as well as an arbitrary
/// `Token` which is used to identify the IO handle in the future.
///
/// ## Edge-triggered and level-triggered
///
/// An IO handle registration may request edge-triggered notifications or
/// level-triggered notifications. This is done by specifying the `PollOpt`
/// argument to `register()` and `reregister()`.
///
/// ## Portability
///
/// Cross platform portability is provided for Mio's TCP & UDP implementations.
///
/// ## Examples
///
/// ```no_run
/// use mio::*;
/// use mio::tcp::*;
/// use std::net::SocketAddr;
///
/// // Construct a new `Poll` handle as well as the `Events` we'll store into
/// let mut poll = Poll::new().unwrap();
/// let mut events = Events::new();
///
/// // Connect the stream
/// let addr: SocketAddr = "173.194.33.80:80".parse().unwrap();
/// let stream = TcpStream::connect(&addr).unwrap();
///
/// // Register the stream with `Poll`
/// poll.register(&stream, Token(0), EventSet::all(), PollOpt::edge()).unwrap();
///
/// // Wait for the socket to become ready
/// poll.poll(&mut events, None).unwrap();
/// ```
pub struct Poll {
    // This type is `Send`, but not `Sync`, so ensure it's exposed as such.
    _marker: marker::PhantomData<Cell<()>>,

    // Platform specific IO selector
    selector: sys::Selector,

    // Custom readiness queue
    readiness_queue: ReadinessQueue,
}

/// Handle to a Poll registration. Used for registering custom types for event
/// notifications.
pub struct Registration {
    inner: RegistrationInner,
}

/// Used to update readiness for an associated `Registration`. `SetReadiness`
/// is `Sync` which allows it to be updated across threads.
#[derive(Clone)]
pub struct SetReadiness {
    inner: RegistrationInner,
}

struct RegistrationInner {
    // ARC pointer to the Poll's readiness queue
    queue: ReadinessQueue,

    // Unsafe pointer to the registration's node. The node is owned by the
    // registration queue.
    node: ReadyRef,
}

#[derive(Clone)]
struct ReadinessQueue {
    inner: Arc<UnsafeCell<ReadinessQueueInner>>,
}

struct ReadinessQueueInner {
    // Used to wake up `Poll` when readiness is set in another thread.
    awakener: sys::Awakener,

    // All readiness nodes are owned by the `Poll` instance and live either in
    // this linked list or in a `readiness_wheel` linked list.
    head_all_nodes: Option<Box<ReadinessNode>>,

    // linked list of nodes that are pending some processing
    head_readiness: AtomicPtr<ReadinessNode>,

    // A fake readiness node used to indicate that `Poll::poll` will block.
    sleep_token: Box<ReadinessNode>,
}

struct ReadyList {
    head: ReadyRef,
}

struct ReadyRef {
    ptr: *mut ReadinessNode,
}

struct ReadinessNode {
    // ===== Fields only accessed by Poll =====
    //
    // Next node in ownership tracking queue
    next_all_nodes: Option<Box<ReadinessNode>>,

    // Previous node in the owned list
    prev_all_nodes: ReadyRef,

    // Data set in register / reregister functions and read in `Poll`. This
    // field should only be accessed from the thread that owns the `Poll`
    // instance.
    registration_data: UnsafeCell<RegistrationData>,

    // ===== Fields accessed by any thread ====
    //
    // Used when the node is queued in the readiness linked list. Accessing
    // this field requires winning the "queue" lock
    next_readiness: ReadyRef,

    // The set of events to include in the notification on next poll
    events: AtomicUsize,

    // Tracks if the node is queued for readiness using the MSB, the
    // rest of the usize is the readiness delay.
    queued: AtomicUsize,

    // Tracks the number of `ReadyRef` pointers
    ref_count: AtomicUsize,
}

struct RegistrationData {
    // The Token used to register the `Evented` with`Poll`
    token: Token,

    // The registration interest
    interest: EventSet,

    // Poll opts
    opts: PollOpt,
}

type Tick = usize;

const NODE_QUEUED_FLAG: usize = 1;

const AWAKEN: Token = Token(usize::MAX);

/*
 *
 * ===== Poll =====
 *
 */

impl Poll {
    /// Return a new `Poll` handle using a default configuration.
    pub fn new() -> io::Result<Poll> {
        let poll = Poll {
            selector: try!(sys::Selector::new()),
            readiness_queue: try!(ReadinessQueue::new()),
            _marker: marker::PhantomData,
        };

        // Register the notification wakeup FD with the IO poller
        try!(poll.readiness_queue.inner().awakener.register(&poll, AWAKEN, EventSet::readable(), PollOpt::edge()));

        Ok(poll)
    }

    /// Register an `Evented` handle with the `Poll` instance.
    pub fn register<E: ?Sized>(&self, io: &E, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        try!(validate_args(token, interest));

        /*
         * Undefined behavior:
         * - Reusing a token with a different `Evented` without deregistering
         * (or closing) the original `Evented`.
         */
        trace!("registering with poller");

        // Register interests for this socket
        try!(io.register(self, token, interest, opts));

        Ok(())
    }

    /// Re-register an `Evented` handle with the `Poll` instance.
    pub fn reregister<E: ?Sized>(&self, io: &E, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()>
        where E: Evented
    {
        try!(validate_args(token, interest));

        trace!("registering with poller");

        // Register interests for this socket
        try!(io.reregister(self, token, interest, opts));

        Ok(())
    }

    /// Deregister an `Evented` handle with the `Poll` instance.
    pub fn deregister<E: ?Sized>(&self, io: &E) -> io::Result<()>
        where E: Evented
    {
        trace!("deregistering IO with poller");

        // Deregister interests for this socket
        try!(io.deregister(self));

        Ok(())
    }

    /// Block the current thread and wait until any `Evented` values registered
    /// with the `Poll` instance are ready or the given timeout has elapsed.
    pub fn poll(&self,
                events: &mut Events,
                timeout: Option<Duration>) -> io::Result<usize> {
        let timeout = if !self.readiness_queue.is_empty() {
            trace!("custom readiness queue has pending events");
            // Never block if the readiness queue has pending events
            Some(Duration::from_millis(0))
        } else if !self.readiness_queue.prepare_for_sleep() {
            Some(Duration::from_millis(0))
        } else {
            timeout
        };

        // First get selector events
        let awoken = try!(self.selector.select(&mut events.inner, AWAKEN,
                                               timeout));

        if awoken {
            self.readiness_queue.inner().awakener.cleanup();
        }

        // Poll custom event queue
        self.readiness_queue.poll(&mut events.inner);

        // Return number of polled events
        Ok(events.len())
    }
}

fn validate_args(token: Token, interest: EventSet) -> io::Result<()> {
    if token == AWAKEN {
        return Err(io::Error::new(io::ErrorKind::Other, "invalid token"));
    }

    if !interest.is_readable() && !interest.is_writable() {
        return Err(io::Error::new(io::ErrorKind::Other, "interest must include readable or writable"));
    }

    Ok(())
}

impl fmt::Debug for Poll {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Poll")
    }
}

/// A buffer for I/O events to get placed into, passed to `Poll::poll`.
///
/// This structure is normally re-used on each turn of the event loop and will
/// contain any I/O events that happen during a `poll`. After a call to `poll`
/// returns the various accessor methods on this structure can be used to
/// iterate over the underlying events that ocurred.
pub struct Events {
    inner: sys::Events,
}

impl Events {
    /// Creates a new blank set of events ready to get passed to `poll`.
    ///
    /// Note that this constructor will attempt to select a "reasonable" default
    /// capacity for the events returned.
    pub fn new() -> Events {
        Events::with_capacity(1024)
    }

    /// Create a net blank set of events capable of holding up to `capacity`
    /// events.
    ///
    /// This parameter typically is an indicator on how many events can be
    /// returned each turn of the event loop, but it is not necessarily a hard
    /// limit across platforms.
    pub fn with_capacity(capacity: usize) -> Events {
        Events {
            inner: sys::Events::with_capacity(capacity),
        }
    }

    /// Returns the `idx`-th event.
    ///
    /// Returns `None` if `idx` is greater than the length of this event buffer.
    pub fn get(&self, idx: usize) -> Option<Event> {
        self.inner.get(idx)
    }

    /// Returns how many events this buffer contains.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns whether this buffer contains 0 events.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// ===== Accessors for internal usage =====

pub fn selector(poll: &Poll) -> &sys::Selector {
    &poll.selector
}

/*
 *
 * ===== Registration =====
 *
 */

impl Registration {
    /// Create a new `Registration` associated with the given `Poll` instance.
    /// The returned `Registration` will be associated with this `Poll` for its
    /// entire lifetime.
    pub fn new(poll: &Poll, token: Token, interest: EventSet, opts: PollOpt) -> (Registration, SetReadiness) {
        let inner = RegistrationInner::new(poll, token, interest, opts);
        let registration = Registration { inner: inner.clone() };
        let set_readiness = SetReadiness { inner: inner.clone() };

        (registration, set_readiness)
    }

    pub fn update(&self, poll: &Poll, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.inner.update(poll, token, interest, opts)
    }

    pub fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.inner.update(poll, Token(0), EventSet::none(), PollOpt::empty())
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let inner = &self.inner;
        inner.registration_data_mut(&inner.queue).unwrap().disable();
    }
}

impl fmt::Debug for Registration {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Registration")
            .finish()
    }
}

unsafe impl Send for Registration { }

impl SetReadiness {
    pub fn readiness(&self) -> EventSet {
        self.inner.readiness()
    }

    pub fn set_readiness(&self, ready: EventSet) -> io::Result<()> {
        self.inner.set_readiness(ready)
    }
}

unsafe impl Send for SetReadiness { }
unsafe impl Sync for SetReadiness { }

impl RegistrationInner {
    fn new(poll: &Poll, token: Token, interest: EventSet, opts: PollOpt) -> RegistrationInner {
        let queue = poll.readiness_queue.clone();
        let node = queue.new_readiness_node(token, interest, opts, 1);

        RegistrationInner {
            node: node,
            queue: queue,
        }
    }

    fn update(&self, poll: &Poll, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        // Update the registration data
        try!(self.registration_data_mut(&poll.readiness_queue)).update(token, interest, opts);

        // If the node is currently ready, re-queue?
        if !event::is_empty(self.readiness()) {
            // The releaxed ordering of `self.readiness()` is sufficient here.
            // All mutations to readiness will immediately attempt to queue the
            // node for processing. This means that this call to
            // `queue_for_processing` is only intended to handle cases where
            // the node was dequeued in `poll` and then has the interest
            // changed, which means that the "newest" readiness value is
            // already known by the current thread.
            let needs_wakeup = self.queue_for_processing();
            debug_assert!(!needs_wakeup, "something funky is going on");
        }

        Ok(())
    }

    fn readiness(&self) -> EventSet {
        // A relaxed ordering is sufficient here as a call to `readiness` is
        // only meant as a hint to what the current value is. It should not be
        // used for any synchronization.
        event::from_usize(self.node().events.load(Ordering::Relaxed))
    }

    fn set_readiness(&self, ready: EventSet) -> io::Result<()> {
        // First store in the new readiness using relaxed as this operation is
        // permitted to be visible ad-hoc. The `queue_for_processing` function
        // will set a `Release` barrier ensuring eventual consistency.
        self.node().events.store(event::as_usize(ready), Ordering::Relaxed);

        trace!("readiness event {:?} {:?}", ready, self.node().token());

        // Setting readiness to none doesn't require any processing by the poll
        // instance, so there is no need to enqueue the node. No barrier is
        // needed in this case since it doesn't really matter when the value
        // becomes visible to other threads.
        if event::is_empty(ready) {
            return Ok(());
        }

        if self.queue_for_processing() {
            try!(self.queue.wakeup());
        }

        Ok(())
    }

    /// Returns true if `Poll` needs to be woken up
    fn queue_for_processing(&self) -> bool {
        // `Release` ensures that the `events` mutation is visible if this
        // mutation is visible.
        //
        // `Acquire` ensures that a change to `head_readiness` made in the
        // poll thread is visible if `queued` has been reset to zero.
        let prev = self.node().queued.compare_and_swap(0, NODE_QUEUED_FLAG, Ordering::AcqRel);

        // If the queued flag was not initially set, then the current thread
        // is assigned the responsibility of enqueuing the node for processing.
        if prev == 0 {
            self.queue.prepend_readiness_node(self.node.clone())
        } else {
            false
        }
    }

    fn node(&self) -> &ReadinessNode {
        self.node.as_ref().unwrap()
    }

    fn registration_data_mut(&self, readiness_queue: &ReadinessQueue) -> io::Result<&mut RegistrationData> {
        // `&Poll` is passed in here in order to ensure that this function is
        // only called from the thread that owns the `Poll` value. This is
        // required because the function will mutate variables that are read
        // from a call to `Poll::poll`.

        if !self.queue.identical(readiness_queue) {
            return Err(io::Error::new(io::ErrorKind::Other, "registration registered with another instance of Poll"));
        }

        Ok(self.node().registration_data_mut())
    }
}

impl Clone for RegistrationInner {
    fn clone(&self) -> RegistrationInner {
        // Using a relaxed ordering is alright here, as knowledge of the
        // original reference prevents other threads from erroneously deleting
        // the object.
        //
        // As explained in the [Boost documentation][1], Increasing the
        // reference counter can always be done with memory_order_relaxed: New
        // references to an object can only be formed from an existing
        // reference, and passing an existing reference from one thread to
        // another must already provide any required synchronization.
        //
        // [1]: (www.boost.org/doc/libs/1_55_0/doc/html/atomic/usage_examples.html)
        let old_size = self.node().ref_count.fetch_add(1, Ordering::Relaxed);

        // However we need to guard against massive refcounts in case someone
        // is `mem::forget`ing Arcs. If we don't do this the count can overflow
        // and users will use-after free. We racily saturate to `isize::MAX` on
        // the assumption that there aren't ~2 billion threads incrementing
        // the reference count at once. This branch will never be taken in
        // any realistic program.
        //
        // We abort because such a program is incredibly degenerate, and we
        // don't care to support it.
        if old_size > MAX_REFCOUNT {
            panic!("too many outstanding refs");
        }

        RegistrationInner {
            queue: self.queue.clone(),
            node: self.node.clone(),
        }
    }
}

impl Drop for RegistrationInner {
    fn drop(&mut self) {
        // Because `fetch_sub` is already atomic, we do not need to synchronize
        // with other threads unless we are going to delete the object. This
        // same logic applies to the below `fetch_sub` to the `weak` count.
        let old_size = self.node().ref_count.fetch_sub(1, Ordering::Release);

        if old_size != 1 {
            return;
        }

        // Signal to the queue that the node is not referenced anymore and can
        // be released / reused
        let _ = self.set_readiness(event::drop());
    }
}

/*
 *
 * ===== ReadinessQueue =====
 *
 */

impl ReadinessQueue {
    fn new() -> io::Result<ReadinessQueue> {
        let sleep_token = Box::new(ReadinessNode::new(Token(0), EventSet::none(), PollOpt::empty(), 0));

        Ok(ReadinessQueue {
            inner: Arc::new(UnsafeCell::new(ReadinessQueueInner {
                awakener: try!(sys::Awakener::new()),
                head_all_nodes: None,
                head_readiness: AtomicPtr::new(ptr::null_mut()),
                // Arguments here don't matter, the node is only used for the
                // pointer value.
                sleep_token: sleep_token,
            }))
        })
    }

    fn poll(&self, dst: &mut sys::Events) {
        let ready = self.take_ready();

        // TODO: Cap number of nodes processed
        for node in ready {
            let mut events;
            let opts;

            {
                let node_ref = node.as_ref().unwrap();
                opts = node_ref.poll_opts();

                // Atomically read queued. Use Acquire ordering to set a
                // barrier before reading events, which will be read using
                // `Relaxed` ordering. Reading events w/ `Relaxed` is OK thanks to
                // the acquire / release hand off on `queued`.
                let mut queued = node_ref.queued.load(Ordering::Acquire);
                events = node_ref.poll_events();

                // Enter a loop attempting to unset the "queued" bit or requeuing
                // the node.
                loop {
                    // In the following conditions, the registration is removed from
                    // the readiness queue:
                    //
                    // - The registration is edge triggered.
                    // - The event set contains no events
                    // - There is a requested delay that has not already expired.
                    //
                    // If the drop flag is set though, the node is never queued
                    // again.
                    if event::is_drop(events) {
                        // dropped nodes are always processed immediately. There is
                        // also no need to unset the queued bit as the node should
                        // not change anymore.
                        break;
                    } else if opts.is_edge() || event::is_empty(events) {
                        // An acquire barrier is set in order to re-read the
                        // `events field. `Release` is not needed as we have not
                        // mutated any field that we need to expose to the producer
                        // thread.
                        let next = node_ref.queued.compare_and_swap(queued, 0, Ordering::Acquire);

                        // Re-read in order to ensure we have the latest value
                        // after having marked the registration has dequeued from
                        // the readiness queue. Again, `Relaxed` is OK since we set
                        // the barrier above.
                        events = node_ref.poll_events();

                        if queued == next {
                            break;
                        }

                        queued = next;
                    } else {
                        // The node needs to stay queued for readiness, so it gets
                        // pushed back onto the queue.
                        //
                        // TODO: It would be better to build up a batch list that
                        // requires a single CAS. Also, `Relaxed` ordering would be
                        // OK here as the prepend only needs to be visible by the
                        // current thread.
                        let needs_wakeup = self.prepend_readiness_node(node.clone());
                        debug_assert!(!needs_wakeup, "something funky is going on");
                        break;
                    }
                }
            }

            // Process the node.
            if event::is_drop(events) {
                // Release the node
                let _ = self.unlink_node(node);
            } else if !events.is_none() {
                let node_ref = node.as_ref().unwrap();

                // TODO: Don't push the event if the capacity of `dst` has
                // been reached
                trace!("readiness event {:?} {:?}", events, node_ref.token());
                dst.push_event(Event::new(events, node_ref.token()));

                // If one-shot, disarm the node
                if opts.is_oneshot() {
                    node_ref.registration_data_mut().disable();
                }
            }
        }
    }

    fn wakeup(&self) -> io::Result<()> {
        self.inner().awakener.wakeup()
    }

    // Attempts to state to sleeping. This involves changing `head_readiness`
    // to `sleep_token`. Returns true if `poll` can sleep.
    fn prepare_for_sleep(&self) -> bool {
        // Use relaxed as no memory besides the pointer is being sent across
        // threads. Ordering doesn't matter, only the current value of
        // `head_readiness`.
        ptr::null_mut() == self.inner().head_readiness
            .compare_and_swap(ptr::null_mut(), self.sleep_token(), Ordering::Relaxed)
    }

    fn take_ready(&self) -> ReadyList {
        // Use `Acquire` ordering to ensure being able to read the latest
        // values of all other atomic mutations.
        let mut head = self.inner().head_readiness.swap(ptr::null_mut(), Ordering::Acquire);

        if head == self.sleep_token() {
            head = ptr::null_mut();
        }

        ReadyList { head: ReadyRef::new(head) }
    }

    fn new_readiness_node(&self, token: Token, interest: EventSet, opts: PollOpt, ref_count: usize) -> ReadyRef {
        let mut node = Box::new(ReadinessNode::new(token, interest, opts, ref_count));
        let ret = ReadyRef::new(&mut *node as *mut ReadinessNode);

        node.next_all_nodes = self.inner_mut().head_all_nodes.take();

        let ptr = &*node as *const ReadinessNode as *mut ReadinessNode;

        if let Some(ref mut next) = node.next_all_nodes {
            next.prev_all_nodes = ReadyRef::new(ptr);
        }

        self.inner_mut().head_all_nodes = Some(node);

        ret
    }

    /// Prepend the given node to the head of the readiness queue. This is done
    /// with relaxed ordering. Returns true if `Poll` needs to be woken up.
    fn prepend_readiness_node(&self, mut node: ReadyRef) -> bool {
        let mut curr_head = self.inner().head_readiness.load(Ordering::Relaxed);

        loop {
            let node_next = if curr_head == self.sleep_token() {
                ptr::null_mut()
            } else {
                curr_head
            };

            // Update next pointer
            node.as_mut().unwrap().next_readiness = ReadyRef::new(node_next);

            // Update the ref, use release ordering to ensure that mutations to
            // previous atomics are visible if the mutation to the head pointer
            // is.
            let next_head = self.inner().head_readiness.compare_and_swap(curr_head, node.ptr, Ordering::Release);

            if curr_head == next_head {
                return curr_head == self.sleep_token();
            }

            curr_head = next_head;
        }
    }

    fn unlink_node(&self, mut node: ReadyRef) -> Box<ReadinessNode> {
        node.as_mut().unwrap().unlink(&mut self.inner_mut().head_all_nodes)
    }

    fn is_empty(&self) -> bool {
        self.inner().head_readiness.load(Ordering::Relaxed).is_null()
    }

    fn sleep_token(&self) -> *mut ReadinessNode {
        &*self.inner().sleep_token as *const ReadinessNode as *mut ReadinessNode
    }

    fn identical(&self, other: &ReadinessQueue) -> bool {
        self.inner.get() == other.inner.get()
    }

    fn inner(&self) -> &ReadinessQueueInner {
        unsafe { mem::transmute(self.inner.get()) }
    }

    fn inner_mut(&self) -> &mut ReadinessQueueInner {
        unsafe { mem::transmute(self.inner.get()) }
    }
}

unsafe impl Send for ReadinessQueue { }

impl ReadinessNode {
    fn new(token: Token, interest: EventSet, opts: PollOpt, ref_count: usize) -> ReadinessNode {
        ReadinessNode {
            next_all_nodes: None,
            prev_all_nodes: ReadyRef::none(),
            registration_data: UnsafeCell::new(RegistrationData::new(token, interest, opts)),
            next_readiness: ReadyRef::none(),
            events: AtomicUsize::new(0),
            queued: AtomicUsize::new(0),
            ref_count: AtomicUsize::new(ref_count),
        }
    }

    fn poll_events(&self) -> EventSet {
        (self.interest() | event::drop()) & event::from_usize(self.events.load(Ordering::Relaxed))
    }

    fn token(&self) -> Token {
        unsafe { &*self.registration_data.get() }.token
    }

    fn interest(&self) -> EventSet {
        unsafe { &*self.registration_data.get() }.interest
    }

    fn poll_opts(&self) -> PollOpt {
        unsafe { &*self.registration_data.get() }.opts
    }

    fn registration_data_mut(&self) -> &mut RegistrationData {
        unsafe { &mut *self.registration_data.get() }
    }

    fn unlink(&mut self, head: &mut Option<Box<ReadinessNode>>) -> Box<ReadinessNode> {
        if let Some(ref mut next) = self.next_all_nodes {
            next.prev_all_nodes = self.prev_all_nodes.clone();
        }

        let node;

        match self.prev_all_nodes.take().as_mut() {
            Some(prev) => {
                node = prev.next_all_nodes.take().unwrap();
                prev.next_all_nodes = self.next_all_nodes.take();
            }
            None => {
                node = head.take().unwrap();
                *head = self.next_all_nodes.take();
            }
        }

        node
    }
}

impl RegistrationData {
    fn new(token: Token, interest: EventSet, opts: PollOpt) -> RegistrationData {
        RegistrationData {
            token: token,
            interest: interest,
            opts: opts,
        }
    }

    fn update(&mut self, token: Token, interest: EventSet, opts: PollOpt) {
        self.token = token;
        self.interest = interest;
        self.opts = opts;
    }

    fn disable(&mut self) {
        self.interest = EventSet::none();
        self.opts = PollOpt::empty();
    }
}

impl Iterator for ReadyList {
    type Item = ReadyRef;

    fn next(&mut self) -> Option<ReadyRef> {
        let mut next = self.head.take();

        if next.is_some() {
            next.as_mut().map(|n| self.head = n.next_readiness.take());
            Some(next)
        } else {
            None
        }
    }
}

impl ReadyRef {
    fn new(ptr: *mut ReadinessNode) -> ReadyRef {
        ReadyRef { ptr: ptr }
    }

    fn none() -> ReadyRef {
        ReadyRef { ptr: ptr::null_mut() }
    }

    fn take(&mut self) -> ReadyRef {
        let ret = ReadyRef { ptr: self.ptr };
        self.ptr = ptr::null_mut();
        ret
    }

    fn is_some(&self) -> bool {
        !self.is_none()
    }

    fn is_none(&self) -> bool {
        self.ptr.is_null()
    }

    fn as_ref(&self) -> Option<&ReadinessNode> {
        if self.ptr.is_null() {
            return None;
        }

        unsafe { Some(&*self.ptr) }
    }

    fn as_mut(&mut self) -> Option<&mut ReadinessNode> {
        if self.ptr.is_null() {
            return None;
        }

        unsafe { Some(&mut *self.ptr) }
    }
}

impl Clone for ReadyRef {
    fn clone(&self) -> ReadyRef {
        ReadyRef::new(self.ptr)
    }
}

impl fmt::Pointer for ReadyRef {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self.as_ref() {
            Some(r) => fmt::Pointer::fmt(&r, fmt),
            None => fmt::Pointer::fmt(&ptr::null::<ReadinessNode>(), fmt),
        }
    }
}

#[cfg(test)]
mod test {
    use {EventSet, Poll, PollOpt, Registration, SetReadiness, Token, Events};
    use std::time::Duration;

    fn ensure_send<T: Send>(_: &T) {}
    fn ensure_sync<T: Sync>(_: &T) {}

    #[allow(dead_code)]
    fn ensure_type_bounds(r: &Registration, s: &SetReadiness) {
        ensure_send(r);
        ensure_send(s);
        ensure_sync(s);
    }

    fn readiness_node_count(poll: &Poll) -> usize {
        let mut cur = poll.readiness_queue.inner().head_all_nodes.as_ref();
        let mut cnt = 0;

        while let Some(node) = cur {
            cnt += 1;
            cur = node.next_all_nodes.as_ref();
        }

        cnt
    }

    #[test]
    pub fn test_nodes_do_not_leak() {
        let mut poll = Poll::new().unwrap();
        let mut events = Events::new();
        let mut registrations = Vec::with_capacity(1_000);

        for _ in 0..3 {
            registrations.push(Registration::new(&mut poll, Token(0), EventSet::readable(), PollOpt::edge()));
        }

        drop(registrations);

        // Poll
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();

        assert_eq!(0, num);
        assert_eq!(0, readiness_node_count(&poll));
    }
}
