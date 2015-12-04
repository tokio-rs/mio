use std::cell::UnsafeCell;
use std::io;
use std::mem;
use std::os::windows::prelude::*;
use std::sync::{Arc, Mutex};
use std::collections::hash_map::{Entry, HashMap};

use slab::Index;
use winapi::*;
use miow;
use miow::iocp::{CompletionPort, CompletionStatus};

use {Token, PollOpt};
use event::{IoEvent, EventSet};
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
    defers: Mutex<Vec<IoEvent>>,

    /// A pool of buffers usable by this selector.
    ///
    /// Primitives will take buffers from this pool to perform I/O operations,
    /// and once complete they'll be put back in.
    buffers: Mutex<BufferPool>,

    /// A list of registered level triggered `IoEvent`s
    level_triggered: Mutex<HashMap<usize, IoEvent>>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        CompletionPort::new(1).map(|cp| {
            Selector {
                inner: Arc::new(SelectorInner {
                    port: cp,
                    defers: Mutex::new(Vec::new()),
                    buffers: Mutex::new(BufferPool::new(256)),
                    level_triggered: Mutex::new(HashMap::new()),
                }),
            }
        })
    }

    pub fn select(&mut self,
                  events: &mut Events,
                  timeout_ms: Option<usize>) -> io::Result<()> {
        // If we have some deferred events then we only want to poll for I/O
        // events, so clamp the timeout to 0 in that case.
        let timeout = if !self.should_block() {
            Some(0)
        } else {
            timeout_ms.map(|ms| ms as u32)
        };

        // Clear out the previous list of I/O events and get some more!
        events.events.truncate(0);
        let inner = self.inner.clone();
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

        for status in events.statuses[..n].iter_mut() {
            if status.overlapped() as usize == 0 {
                dst.push(IoEvent::new(EventSet::readable(),
                                      Token(status.token())));
                continue
            }

            let callback = unsafe {
                (*(status.overlapped() as *mut Overlapped)).callback()
            };

            callback(status, dst);
        }

        // Clear out the list of deferred events and process them all
        // here.
        let defers = mem::replace(&mut *inner.defers.lock().unwrap(), Vec::new());

        for event in defers {
            dst.push(event);
        }

        // Finally, push all level triggered events
        for event in inner.level_triggered.lock().unwrap().values() {
            dst.push(*event);
        }

        Ok(())
    }

    fn should_block(&self) -> bool {
        if !self.inner.defers.lock().unwrap().is_empty() {
            return false;
        }

        self.inner.level_triggered.lock().unwrap().is_empty()
    }
}

pub struct Registration {
    key: Option<usize>,
    selector: Option<Arc<SelectorInner>>,
    token: Token,
    opts: PollOpt,
    interest: EventSet,
}

impl Registration {
    pub fn new() -> Registration {
        Registration {
            key: None,
            selector: None,
            token: Token(0),
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

    pub fn token(&self) -> Token { self.token }

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
    /// translate that to an `IoEvent` and process accordingly.
    ///
    /// This function will mask out all ignored events (e.g. ignore `writable`
    /// events if they weren't requested) and also handle properties such as
    /// `oneshot`.
    ///
    /// Eventually this function will probably also be modified to handle the
    /// `level()` polling option.
    pub fn push_event(&mut self, set: EventSet, events: &mut Vec<IoEvent>) {
        trace!("push_event; token={:?}; set={:?}; opts={:?}", self.token, set, self.opts);

        // If we're not actually interested in any of these events,
        // discard the event, and then if we're actually delivering an event we
        // stop listening if it's also a oneshot.
        let set = self.interest & set;

        if set != EventSet::none() {
            let event = IoEvent::new(set, self.token);

            if self.opts.is_edge() {
                events.push(event);

                if self.opts.is_oneshot() {
                    trace!("deregistering because of oneshot");
                    self.interest = EventSet::none();
                }
            } else {
                let selector = self.selector.as_ref()
                    .expect("expected a selector");

                let mut level = selector.level_triggered.lock().unwrap();

                match level.entry(self.key.expect("expected registration key")) {
                    Entry::Occupied(mut e) => {
                        let e = e.get_mut();
                        debug_assert!(e.token == self.token);
                        e.kind = e.kind | event.kind;
                    }
                    Entry::Vacant(e) => {
                        e.insert(event);
                    }
                }
            }
        }
    }

    pub fn unset_readiness(&mut self, set: EventSet) {
        trace!("unset_readiness; token={:?}; set={:?}", self.token, set);

        if let Some(key) = self.key {
            let mut map = self.selector.as_ref().expect("expected selector")
                .level_triggered.lock().unwrap();

            if let Entry::Occupied(mut e) = map.entry(key) {
                {
                    let event = e.get_mut();
                    event.kind = event.kind & !set;
                }

                if e.get().kind == EventSet::none() {
                    e.remove();
                }
            }
        }
    }

    pub fn associate(&mut self, selector: &mut Selector, token: Token) {
        self.selector = Some(selector.inner.clone());
        self.token = token;
    }

    pub fn register_socket(&mut self,
                           socket: &AsRawSocket,
                           selector: &mut Selector,
                           token: Token,
                           interest: EventSet,
                           opts: PollOpt) -> io::Result<()> {
        if self.selector.is_some() {
            return Err(other("socket already registered"))
        }

        try!(Registration::validate_opts(opts));
        try!(selector.inner.port.add_socket(self.token.as_usize(), socket));
        self.associate(selector, token);

        if opts.is_level() {
            self.key = Some(socket.as_raw_socket() as usize);
        }

        self.interest = set2mask(interest);
        self.opts = opts;
        Ok(())
    }

    pub fn reregister_socket(&mut self,
                             _socket: &AsRawSocket,
                             _selector: &mut Selector,
                             token: Token,
                             interest: EventSet,
                             opts: PollOpt) -> io::Result<()> {
        if self.selector.is_none() {
            return Err(other("socket not registered"))
        } else if self.token != token {
            return Err(other("cannot change token values on reregistration"))
        }
        try!(Registration::validate_opts(opts));
        // TODO: assert that self.selector == selector?

        self.interest = set2mask(interest);

        // Reset any queued level events
        if self.key.is_some() {
            self.unset_readiness(!interest);
        }

        self.opts = opts;
        Ok(())
    }

    pub fn deregister(&mut self) {
        trace!("deregister; token={:?}", self.token);

        if let Some(key) = self.key {
            self.key = None;
            self.selector.as_ref().expect("expected selector")
                .level_triggered.lock().unwrap().remove(&key);
        }
    }

    pub fn checked_deregister(&mut self, selector: &Selector) -> io::Result<()> {
        match self.selector {
            Some(ref s) => {
                let inner1: &SelectorInner = &*selector.inner;
                let inner2: &SelectorInner = &*s;

                if inner1 as *const SelectorInner != inner2 as *const SelectorInner {
                    return Err(other("socket registered with other selector"));
                }
            }
            None => {
                return Err(super::bad_state());
            }
        }

        self.deregister();
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
            let mut dst = s.defers.lock().unwrap();
            self.push_event(set, &mut dst);
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
    events: Vec<IoEvent>,
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

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn get(&self, idx: usize) -> IoEvent {
        self.events[idx]
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

pub type Callback = fn(&CompletionStatus, &mut Vec<IoEvent>);

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
