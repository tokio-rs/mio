use std::cell::UnsafeCell;
use std::collections::hash_map::{HashMap, Entry};
use std::io;
use std::mem;
use std::os::windows::prelude::*;
use std::sync::{Arc, Mutex};

use slab::Index;
use winapi::*;
use wio;
use wio::iocp::{CompletionPort, CompletionStatus};

use {Token, PollOpt};
use event::{IoEvent, EventSet};
use sys::windows::from_raw_arc::FromRawArc;

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

pub struct SelectorInner {
    /// The actual completion port that's used to manage all I/O
    port: CompletionPort,

    /// A list of all registered handles with this selector.
    ///
    /// The key of this map is either a `SOCKET` or a `HANDLE`, and the value is
    /// `None` if the handle was registered with `oneshot` and it expired, or
    /// `Some` if the handle is registered and receiving events.
    handles: Mutex<HashMap<usize, Option<Registration>>>,

    /// A list of deferred events to be generated on the next call to `select`.
    ///
    /// Events can sometimes be generated without an associated I/O operation
    /// having completed, and this list is emptied out and returned on each turn
    /// of the event loop.
    defers: Mutex<Vec<(usize, EventSet, Token)>>,
}

pub type Callback = fn(&CompletionStatus, &mut FnMut(HANDLE, EventSet));

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
    inner: UnsafeCell<wio::Overlapped>,
    callback: Callback,
}

#[derive(Copy, Clone)]
struct Registration {
    token: Token,
    opts: PollOpt,
    interest: EventSet,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        CompletionPort::new(1).map(|cp| {
            Selector {
                inner: Arc::new(SelectorInner {
                    port: cp,
                    handles: Mutex::new(HashMap::new()),
                    defers: Mutex::new(Vec::new()),
                }),
            }
        })
    }

    pub fn select(&mut self,
                  events: &mut Events,
                  timeout_ms: usize) -> io::Result<()> {
        // If we have some deferred events then we only want to poll for I/O
        // events, so clamp the timeout to 0 in that case.
        let timeout = if self.inner.defers.lock().unwrap().len() > 0 {
            0
        } else {
            timeout_ms as u32
        };

        // Clear out the previous list of I/O events and get some more!
        events.events.truncate(0);
        let inner = self.inner.clone();
        let n = match inner.port.get_many(&mut events.statuses, Some(timeout)) {
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
                (*(status.overlapped() as *mut Overlapped)).callback
            };
            callback(status, &mut |handle, set| {
                inner.push_event(dst, handle, set, Token(status.token()));
            });
        }

        // Finally, clear out the list of deferred events and process them all
        // here.
        let defers = mem::replace(&mut *inner.defers.lock().unwrap(), Vec::new());
        for (handle, set, token) in defers {
            inner.push_event(dst, handle as HANDLE, set, token);
        }
        Ok(())
    }

    pub fn inner(&self) -> &Arc<SelectorInner> { &self.inner }
}

impl SelectorInner {
    pub fn port(&self) -> &CompletionPort { &self.port }

    /// Given a handle, token, and an event set describing how its ready,
    /// translate that to an `IoEvent` and process accordingly.
    ///
    /// This function will mask out all ignored events (e.g. ignore `writable`
    /// events if they weren't requested) and also handle properties such as
    /// `oneshot`.
    ///
    /// Eventually this function will probably also be modified to handle the
    /// `level()` polling option.
    fn push_event(&self, events: &mut Vec<IoEvent>, handle: HANDLE,
                  set: EventSet, token: Token) {
        // A vacant handle means it's been deregistered, so just skip this
        // event.
        let mut handles = self.handles.lock().unwrap();
        let mut e = match handles.entry(handle as usize) {
            Entry::Vacant(..) => return,
            Entry::Occupied(e) => e,
        };

        // A handle in the map without a registration is one that's become idle
        // as a result of a `oneshot`, so just use a registration that will turn
        // this function into a noop.
        let reg = e.get().unwrap_or(Registration {
            token: Token(0),
            interest: EventSet::none(),
            opts: PollOpt::oneshot(),
        });

        // If we're not actually interested in any of these events,
        // discard the event, and then if we're actually delivering an event we
        // stop listening if it's also a oneshot.
        let set = reg.interest & set;
        if set != EventSet::none() {
            events.push(IoEvent::new(set, token));

            if reg.opts.is_oneshot() {
                trace!("deregistering because of oneshot");
                e.insert(None);
            }
        }
    }

    pub fn register_socket(&self, socket: &AsRawSocket, token: Token,
                           interest: EventSet, opts: PollOpt)
                           -> io::Result<()> {
        if opts.contains(PollOpt::level()) {
            return Err(io::Error::new(io::ErrorKind::Other,
                                      "level opt not implemented on windows"))
        } else if !opts.contains(PollOpt::edge()) {
            return Err(other("must have edge opt"))
        }

        let mut handles = self.handles.lock().unwrap();
        match handles.entry(socket.as_raw_socket() as usize) {
            Entry::Occupied(..) => return Err(other("socket already registered")),
            Entry::Vacant(v) => {
                try!(self.port.add_socket(token.as_usize(), socket));
                v.insert(Some(Registration {
                    token: token,
                    interest: set2mask(interest),
                    opts: opts,
                }));
            }
        }

        Ok(())
    }

    pub fn reregister_socket(&self, socket: &AsRawSocket, token: Token,
                             interest: EventSet, opts: PollOpt)
                             -> io::Result<()> {
        if opts.contains(PollOpt::level()) {
            return Err(io::Error::new(io::ErrorKind::Other,
                                      "level opt not implemented on windows"))
        } else if !opts.contains(PollOpt::edge()) {
            return Err(other("must have edge opt"))
        }

        let mut handles = self.handles.lock().unwrap();
        match handles.entry(socket.as_raw_socket() as usize) {
            Entry::Vacant(..) => return Err(other("socket not registered")),
            Entry::Occupied(mut v) => {
                match v.get().as_ref().map(|t| t.token) {
                    Some(t) if t == token => {}
                    Some(..) => return Err(other("cannot change tokens")),
                    None => {}
                }
                v.insert(Some(Registration {
                    token: token,
                    interest: set2mask(interest),
                    opts: opts,
                }));
            }
        }

        Ok(())
    }

    pub fn deregister_socket(&self, socket: &AsRawSocket) -> io::Result<()> {
        // Note that we can't actually deregister the socket from the completion
        // port here, so we just remove our own internal metadata about it.
        let mut handles = self.handles.lock().unwrap();
        match handles.entry(socket.as_raw_socket() as usize) {
            Entry::Vacant(..) => return Err(other("socket not registered")),
            Entry::Occupied(v) => { v.remove(); }
        }
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
    pub fn defer(&self, handle: HANDLE, set: EventSet) {
        debug!("defer {:?} {:?}", handle, set);
        let handles = self.handles.lock().unwrap();
        let reg = handles.get(&(handle as usize)).and_then(|t| t.as_ref())
                         .map(|t| t.token);
        let token = match reg {
            Some(token) => token,
            None => return,
        };
        self.defers.lock().unwrap().push((handle as usize, set, token));
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

impl Overlapped {
    pub fn new(cb: Callback) -> Overlapped {
        Overlapped {
            inner: UnsafeCell::new(wio::Overlapped::zero()),
            callback: cb,
        }
    }

    pub unsafe fn get_mut(&self) -> &mut wio::Overlapped {
        &mut *self.inner.get()
    }

    pub unsafe fn cast_to_arc<T>(overlapped: *mut wio::Overlapped,
                                 offset: usize) -> FromRawArc<T> {
        debug_assert!(offset < mem::size_of::<T>());
        FromRawArc::from_raw((overlapped as usize - offset) as *mut T)
    }
}
