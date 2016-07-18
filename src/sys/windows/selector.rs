use std::{fmt, io, mem};
use std::cell::UnsafeCell;
use std::os::windows::prelude::*;
use std::sync::{Arc, Mutex};

use winapi::*;
use miow;
use miow::iocp::{CompletionPort, CompletionStatus};

use {Token, PollOpt};
use event::{Event, EventSet};
use sys::windows::from_raw_arc::FromRawArc;
use sys::windows::buffer_pool::BufferPool;

/// The guts of the Windows event loop, this is the struct which actually owns
/// a completion port.
///
/// Internally this is just an `Arc`, and this allows handing out references to
/// the internals to I/O handles registered on this selector. This is
/// required to schedule I/O operations independently of being inside the event
/// loop (e.g. when a call to `write` is seen we're not "in the event loop").
pub struct Selector {
    inner: Arc<SelectorInner>,
}

struct SelectorInner {
    /// The actual completion port that's used to manage all I/O
    port: CompletionPort,

    /// A list of deferred events to be generated on the next call to `select`.
    ///
    /// Events can sometimes be generated without an associated I/O operation
    /// having completed, and this list is emptied out and returned on each turn
    /// of the event loop.
    pending: Mutex<Vec<EventRef>>,

    /// A pool of buffers usable by this selector.
    ///
    /// Primitives will take buffers from this pool to perform I/O operations,
    /// and once complete they'll be put back in.
    buffers: Mutex<BufferPool>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        CompletionPort::new(1).map(|cp| {
            Selector {
                inner: Arc::new(SelectorInner {
                    port: cp,
                    buffers: Mutex::new(BufferPool::new(256)),
                    pending: Mutex::new(Vec::new()),
                }),
            }
        })
    }

    pub fn select(&mut self, events: &mut Events, awakener: Token, timeout_ms: Option<usize>) -> io::Result<bool> {
        let mut ret = false;

        // If we have some deferred events then we only want to poll for I/O
        // events, so clamp the timeout to 0 in that case.
        let timeout = if !self.should_block() {
            Some(0)
        } else {
            timeout_ms.map(|ms| ms as u32)
        };

        trace!("select; timeout={:?}", timeout);

        // Clear out the previous list of I/O events and get some more!
        events.events.truncate(0);
        let inner = &*self.inner;

        trace!("polling IOCP");
        let n = match inner.port.get_many(&mut events.statuses, timeout) {
            Ok(statuses) => statuses.len(),
            Err(ref e) if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) => 0,
            Err(e) => return Err(e),
        };

        // First up, process all completed I/O events. Lookup the callback
        // associated with the I/O and invoke it. Also, carefully don't hold any
        // locks while we invoke a callback in case more I/O is scheduled to
        // prevent deadlock.
        //
        // Note that if we see an I/O completion with a null OVERLAPPED pointer
        // then it means it was our awakener, so just generate a readable
        // notification for it and carry on.
        let dst = &mut events.events;

        // Clear out the list of pending events and process them all
        // here.
        trace!("select; locking mutex");
        let mut pending = inner.pending.lock().unwrap();

        for status in events.statuses[..n].iter_mut() {
            if status.overlapped() as usize == 0 {
                if Token(status.token()) == awakener {
                    ret = true;
                    continue;
                }

                dst.push(Event::new(EventSet::readable(),
                                    Token(status.token())));
                continue;
            }

            let callback = unsafe {
                (*(status.overlapped() as *mut Overlapped)).callback()
            };

            trace!("select; -> got overlapped");
            callback(status, &mut *pending);
        }

        // TODO: improve
        for event in mem::replace(&mut *pending, Vec::new()) {
            trace!("polled event; event={:?}", event);

            if !event.is_none() {
                if event.token() == awakener {
                    ret = true;
                } else {
                    dst.push(event.as_event());

                    if event.is_level() {
                        pending.push(event);
                    } else {
                        event.unset_pending();
                    }
                }
            } else {
                event.unset_pending();
            }
        }

        trace!("returning");
        Ok(ret)
    }

    fn should_block(&self) -> bool {
        trace!("should_block; locking mutex");
        self.inner.pending.lock().unwrap().is_empty()
    }
}

impl SelectorInner {
    fn identical(&self, other: &SelectorInner) -> bool {
        (self as *const SelectorInner) == (other as *const SelectorInner)
    }
}

// EventInner because we want access to the inner fields
struct EventInner {
    token: Token,
    kind: EventSet,
    pending: bool,
    level: bool,
}

#[derive(Clone)]
pub struct EventRef {
    inner: Arc<UnsafeCell<EventInner>>,
}

impl EventRef {
    fn token(&self) -> Token {
        self.inner().token
    }

    fn is_pending(&self) -> bool {
        self.inner().pending
    }

    fn is_none(&self) -> bool {
        self.inner().kind.is_none()
    }

    fn is_level(&self) -> bool {
        self.inner().level
    }

    fn associate(&self, token: Token, opts: PollOpt) {
        let inner = self.mut_inner();
        inner.token = token;
        inner.level = opts.is_level();
    }

    fn set_pending(&self) {
        self.mut_inner().pending = true;
    }

    fn unset_pending(&self) {
        self.mut_inner().pending = false;
    }

    fn update(&self, interest: EventSet, events: EventSet) -> EventSet {
        let curr = interest & (self.inner().kind | events);
        self.mut_inner().kind = curr;
        curr
    }

    fn unset(&self, events: EventSet) {
        let curr = self.inner().kind & !events;
        self.mut_inner().kind = curr;
    }

    fn inner(&self) -> &EventInner {
        unsafe { &*self.inner.get() }
    }

    fn mut_inner(&self) -> &mut EventInner {
        unsafe { &mut *self.inner.get() }
    }

    fn as_event(&self) -> Event {
        let inner = self.inner();
        Event::new(inner.kind, inner.token)
    }
}

impl fmt::Debug for EventRef {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let i = self.inner();
        fmt.debug_struct("EventRef")
            .field("token", &i.token)
            .field("kind", &i.kind)
            .field("pending", &i.pending)
            .field("level", &i.level)
            .finish()
    }
}

unsafe impl Send for EventRef {}
unsafe impl Sync for EventRef {}

pub struct Registration {
    selector: Option<Arc<SelectorInner>>,
    event: EventRef,
    opts: PollOpt,
    interest: EventSet,
}

impl Registration {
    pub fn new() -> Registration {
        Registration {
            selector: None,
            event: EventRef {
                inner: Arc::new(UnsafeCell::new(EventInner {
                    token: Token(0),
                    kind: EventSet::none(),
                    pending: false,
                    level: false,
                }))
            },
            opts: PollOpt::empty(),
            interest: EventSet::none(),
        }
    }

    fn validate_opts(opts: PollOpt) -> io::Result<()> {
        if !opts.contains(PollOpt::edge()) && !opts.contains(PollOpt::level()) {
            return Err(other("must have edge or level opt"));
        }

        Ok(())
    }

    pub fn port(&self) -> Option<&CompletionPort> {
        self.selector.as_ref().map(|s| &s.port)
    }

    pub fn token(&self) -> Token {
        self.event.token()
    }

    pub fn get_buffer(&self, size: usize) -> Vec<u8> {
        match self.selector {
            Some(ref s) => s.buffers.lock().unwrap().get(size),
            None => Vec::with_capacity(size),
        }
    }

    pub fn put_buffer(&self, buf: Vec<u8>) {
        if let Some(ref s) = self.selector {
            s.buffers.lock().unwrap().put(buf);
        }
    }

    /// Given a handle, token, and an event set describing how its ready,
    /// translate that to an `Event` and process accordingly.
    ///
    /// This function will mask out all ignored events (e.g. ignore `writable`
    /// events if they weren't requested) and also handle properties such as
    /// `oneshot`.
    ///
    /// Eventually this function will probably also be modified to handle the
    /// `level()` polling option.
    pub fn push_event(&mut self, set: EventSet, events: &mut Vec<EventRef>) {
        trace!("push_event; set={:?}; self.event={:?}", set, self.event);
        // The lock on EventInner is currently held, update interest
        let curr = self.event.update(self.interest, set);

        if !curr.is_none() {
            if !self.event.is_pending() {
                self.event.set_pending();
                events.push(self.event.clone());
                trace!("pushing event; event={:?}", self.event);
            }

            if self.opts.is_oneshot() {
                trace!("deregistering because of oneshot");
                self.interest = EventSet::none();
            }
        } else {
            trace!("   -> is_none");
        }
    }

    pub fn unset_readiness(&mut self, set: EventSet, need_lock: bool) {
        trace!("unset_readiness; locking mutex");
        // Acquire the lock if needed

        let _lock = if need_lock {
            Some(self.selector.as_ref()
                .map(|s| s.pending.lock().unwrap()))
        } else {
            None
        };

        self.event.unset(set);
    }

    pub fn associate(&mut self, selector: &Selector, token: Token, opts: PollOpt) -> io::Result<()> {
        // Structured like this to make the borrow checker happy
        if self.selector.is_some() {
            if let Some(sa) = self.selector.as_ref() {
                if !selector.inner.identical(&**sa) {
                    return Err(other("socket already registered"));
                }
            }
        } else {
            self.selector = Some(selector.inner.clone());
        }

        self.event.associate(token, opts);
        Ok(())
    }

    pub fn register_socket(&mut self,
                           socket: &AsRawSocket,
                           selector: &Selector,
                           token: Token,
                           interest: EventSet,
                           opts: PollOpt) -> io::Result<()> {

        try!(Registration::validate_opts(opts));
        try!(self.associate(selector, token, opts));
        try!(selector.inner.port.add_socket(usize::from(token), socket));

        self.interest = set2mask(interest);
        self.opts = opts;
        Ok(())
    }

    pub fn reregister_socket(&mut self,
                             _socket: &AsRawSocket,
                             selector: &Selector,
                             token: Token,
                             interest: EventSet,
                             opts: PollOpt) -> io::Result<()> {

        if self.selector.is_none() {
            return Err(other("socket not registered"));
        }

        try!(Registration::validate_opts(opts));
        try!(self.associate(selector, token, opts));

        trace!("reregister_socket; interest={:?}", set2mask(interest));
        self.interest = set2mask(interest);
        self.unset_readiness(!interest, false);

        self.opts = opts;
        Ok(())
    }

    pub fn deregister(&mut self, need_lock: bool) {
        self.unset_readiness(EventSet::all(), need_lock);
    }

    pub fn checked_deregister(&mut self, selector: &Selector) -> io::Result<()> {
        match self.selector {
            Some(ref s) => {
                if !s.identical(&*selector.inner) {
                    return Err(other("socket registered with other selector"));
                }
            }
            None => {
                return Err(super::bad_state());
            }
        }

        // This is always called from the event loop thread
        self.deregister(false);
        Ok(())
    }

    /// Schedules some events for a handle to be delivered on the next turn of
    /// the event loop (without an associated I/O event).
    ///
    /// This function will discard this if:
    ///
    /// * The handle has been de-registered
    /// * The handle doesn't have an active registration (e.g. its oneshot
    ///   expired)
    pub fn defer(&mut self, set: EventSet) {
        if let Some(s) = self.selector.clone() {
            trace!("defer; locking mutex");
            let mut dst = s.pending.lock().unwrap();
            self.push_event(set, &mut *dst);
        }
    }
}

/// From a given interest set return the event set mask used to generate events.
///
/// The only currently interesting thing this function does is ensure that hup
/// events are generated for interests that only include the readable event.
fn set2mask(e: EventSet) -> EventSet {
    if e.is_readable() {
        e | EventSet::hup()
    } else {
        e
    }
}

fn other(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, s)
}

#[derive(Debug)]
pub struct Events {
    /// Raw I/O event completions are filled in here by the call to `get_many`
    /// on the completion port above. These are then postprocessed into the
    /// vector below.
    statuses: Box<[CompletionStatus]>,

    /// Literal events returned by `get` to the upwards `EventLoop`
    events: Vec<Event>,
}

impl Events {
    pub fn new() -> Events {
        // Use a nice large space for receiving I/O events (currently the same
        // as unix's 1024) and then also prepare the output vector to have the
        // same space.
        //
        // Note that it's possible for the output `events` to grow beyond 1024
        // capacity as it can also include deferred events, but that's certainly
        // not the end of the world!
        Events {
            statuses: vec![CompletionStatus::zero(); 1024].into_boxed_slice(),
            events: Vec::with_capacity(1024),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn get(&self, idx: usize) -> Option<Event> {
        self.events.get(idx).map(|e| *e)
    }

    pub fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }
}

macro_rules! overlapped2arc {
    ($e:expr, $t:ty, $($field:ident).+) => (
        ::sys::windows::selector::Overlapped::cast_to_arc::<$t>($e,
                offset_of!($t, $($field).+))
    )
}

macro_rules! offset_of {
    ($t:ty, $($field:ident).+) => (
        &(*(0 as *const $t)).$($field).+ as *const _ as usize
    )
}

pub type Callback = fn(&CompletionStatus, &mut Vec<EventRef>);

/// See sys::windows module docs for why this exists.
///
/// The gist of it is that `Selector` assumes that all `OVERLAPPED` pointers are
/// actually inside one of these structures so it can use the `Callback` stored
/// right after it.
///
/// We use repr(C) here to ensure that we can assume the overlapped pointer is
/// at the start of the structure so we can just do a cast.
#[repr(C)]
pub struct Overlapped {
    inner: UnsafeCell<miow::Overlapped>,
    callback: Callback,
}

impl Overlapped {
    pub fn new(cb: Callback) -> Overlapped {
        Overlapped {
            inner: UnsafeCell::new(miow::Overlapped::zero()),
            callback: cb,
        }
    }

    pub unsafe fn get_mut(&self) -> &mut miow::Overlapped {
        &mut *self.inner.get()
    }

    pub unsafe fn cast_to_arc<T>(overlapped: *mut miow::Overlapped,
                                 offset: usize) -> FromRawArc<T> {
        debug_assert!(offset < mem::size_of::<T>());
        FromRawArc::from_raw((overlapped as usize - offset) as *mut T)
    }

    pub unsafe fn callback(&self) -> &Callback {
        &self.callback
    }
}

// Overlapped's APIs are marked as unsafe Overlapped's APIs are marked as
// unsafe as they must be used with caution to ensure thread safety. The
// structure itself is safe to send across threads.
unsafe impl Send for Overlapped {}
unsafe impl Sync for Overlapped {}
