use {io, Event, PollOpt, Ready, Token};
use sys::fuchsia::{
    epoll_event_to_ready,
    poll_opts_to_wait_async,
    status_to_io_err,
    EventedFd,
    EventedFdInner,
};
use magenta;
use magenta_sys;
use magenta::HandleBase;
use std::collections::hash_map;
use std::fmt;
use std::mem;
use std::sync::atomic::{AtomicBool, AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use sys;

/// The kind of registration-- file descriptor or handle.
///
/// The last bit of a token is set to indicate the type of the registration.
#[derive(Copy, Clone, Eq, PartialEq)]
enum RegType {
    Fd,
    Handle,
}

fn key_from_token_and_type(token: Token, reg_type: RegType) -> u64 {
    let key = token.0 as u64;
    let msb = 1u64 << 63;
    if (key & msb) != 0 {
        panic!("Most-significant bit of token must remain unset.");
    }

    match reg_type {
        RegType::Fd => key,
        RegType::Handle => key | msb,
    }
}

fn token_and_type_from_key(key: u64) -> (Token, RegType) {
    let msb = 1u64 << 63;
    (
        Token((key & !msb) as usize),
        if (key & msb) == 0 {
            RegType::Fd
        } else {
            RegType::Handle
        }
    )
}

/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associated with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
static NEXT_ID: AtomicUsize = ATOMIC_USIZE_INIT;

pub struct Selector {
    id: usize,

    /// Magenta object on which the handles have been registered, and on which events occur
    port: Arc<magenta::Port>,

    /// Whether or not `tokens_to_rereg` contains any elements. This is a best-effort attempt
    /// used to prevent having to lock `tokens_to_rereg` when it is empty.
    has_tokens_to_rereg: AtomicBool,

    /// List of `Token`s corresponding to registrations that need to be reregistered before the
    /// next `port::wait`. This is necessary to provide level-triggered behavior for
    /// `Async::repeating` registrations.
    ///
    /// When a level-triggered `Async::repeating` event is seen, its token is added to this list so
    /// that it will be reregistered before the next `port::wait` call, making `port::wait` return
    /// immediately if the signal was high during the reregistration.
    ///
    /// Note: when used at the same time, the `tokens_to_rereg` lock should be taken out _before_
    /// `token_to_fd`.
    tokens_to_rereg: Mutex<Vec<Token>>,

    /// Map from tokens to weak references to `EventedFdInner`-- a structure describing a
    /// file handle, its associated `mxio` object, and its current registration.
    token_to_fd: Mutex<hash_map::HashMap<Token, Weak<EventedFdInner>>>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let port = Arc::new(
            magenta::Port::create(magenta::PortOpts::Default)
                .map_err(status_to_io_err)?
        );

        // offset by 1 to avoid choosing 0 as the id of a selector
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

        let has_tokens_to_rereg = AtomicBool::new(false);
        let tokens_to_rereg = Mutex::new(Vec::new());
        let token_to_fd = Mutex::new(hash_map::HashMap::new());

        Ok(Selector {
            id,
            port,
            has_tokens_to_rereg,
            tokens_to_rereg,
            token_to_fd,
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Returns a reference to the underlying port `Arc`.
    pub fn port(&self) -> &Arc<magenta::Port> { &self.port }

    /// Reregisters all registrations pointed to by the `tokens_to_rereg` list
    /// if `has_tokens_to_rereg`.
    fn reregister_handles(&self) -> io::Result<()> {
        if self.has_tokens_to_rereg.load(Ordering::Relaxed) {
            let mut tokens = self.tokens_to_rereg.lock().unwrap();
            let token_to_fd = self.token_to_fd.lock().unwrap();
            for token in tokens.drain(0..) {
                if let Some(eventedfd) = token_to_fd.get(&token)
                    .and_then(|h| h.upgrade()) {
                    eventedfd.rereg_for_level(&self.port);
                }
            }
            self.has_tokens_to_rereg.store(false, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn select(&self,
                  evts: &mut Events,
                  _awakener: Token,
                  timeout: Option<Duration>) -> io::Result<bool>
    {
        evts.event_opt = None;

        self.reregister_handles()?;

        let deadline = match timeout {
            Some(duration) => {
                let nanos = (duration.as_secs() * 1_000_000_000) +
                    (duration.subsec_nanos() as u64);

                magenta::deadline_after(nanos)
            }
            None => magenta::MX_TIME_INFINITE,
        };

        let packet = match self.port.wait(deadline) {
            Ok(packet) => packet,
            Err(magenta::Status::ErrTimedOut) => return Ok(false),
            Err(e) => return Err(status_to_io_err(e)),
        };

        let observed_signals = match packet.contents() {
            magenta::PacketContents::SignalOne(signal_packet) => {
                signal_packet.observed()
            }
            magenta::PacketContents::SignalRep(signal_packet) => {
                signal_packet.observed()
            }
            magenta::PacketContents::User(_user_packet) => {
                // User packets are only ever sent by an Awakener
                return Ok(true);
            }
        };

        let key = packet.key();
        let (token, reg_type) = token_and_type_from_key(key);

        match reg_type {
            RegType::Handle => {
                // We can return immediately-- no lookup or registration necessary.
                evts.event_opt = Some(Event::new(signals_to_ready(observed_signals), token));
                Ok(false)
            },
            RegType::Fd => {
                // Convert the signals to epoll events using __mxio_wait_end,
                // and add to reregistration list if necessary.
                let events: u32;
                {
                    let handle = if let Some(handle) =
                    self.token_to_fd.lock().unwrap()
                        .get(&token)
                        .and_then(|h| h.upgrade()) {
                        handle
                    } else {
                        // This handle is apparently in the process of removal.
                        // It has been removed from the list, but port_cancel has not been called.
                        return Ok(false);
                    };

                    events = unsafe {
                        let mut events: u32 = mem::uninitialized();
                        sys::fuchsia::sys::__mxio_wait_end(handle.mxio, observed_signals, &mut events);
                        events
                    };

                    // If necessary, queue to be reregistered before next port_await
                    let needs_to_rereg = {
                        let registration_lock = handle.registration.lock().unwrap();

                        registration_lock
                            .as_ref()
                            .map(|r| &r.rereg_signals)
                            .is_some()
                    };

                    if needs_to_rereg {
                        let mut tokens_to_rereg_lock = self.tokens_to_rereg.lock().unwrap();
                        tokens_to_rereg_lock.push(token);
                        self.has_tokens_to_rereg.store(true, Ordering::Relaxed);
                    }
                }

                evts.event_opt = Some(Event::new(epoll_event_to_ready(events), token));
                Ok(false)
            },
        }
    }

    /// Register event interests for the given IO handle with the OS
    pub ( in sys::fuchsia) fn register_fd(&self,
                                          handle: &magenta::Handle,
                                          fd: &EventedFd,
                                          token: Token,
                                          signals: magenta::Signals,
                                          poll_opts: PollOpt) -> io::Result<()>
    {
        {
            let mut token_to_fd = self.token_to_fd.lock().unwrap();
            match token_to_fd.entry(token) {
                hash_map::Entry::Occupied(_) =>
                    return Err(io::Error::new(io::ErrorKind::AlreadyExists,
                               "Attempted to register a filedescriptor on an existing token.")),
                hash_map::Entry::Vacant(slot) => slot.insert(Arc::downgrade(&fd.inner)),
            };
        }

        let wait_async_opts = poll_opts_to_wait_async(poll_opts);

        let wait_res = handle.wait_async(&self.port, token.0 as u64, signals, wait_async_opts)
            .map_err(status_to_io_err);

        if wait_res.is_err() {
            self.token_to_fd.lock().unwrap().remove(&token);
        }

        wait_res
    }

    /// Deregister event interests for the given IO handle with the OS
    pub ( in sys::fuchsia) fn deregister_fd(&self,
                                            handle: &magenta::Handle,
                                            token: Token) -> io::Result<()> {
        self.token_to_fd.lock().unwrap().remove(&token);

        // We ignore NotFound errors since oneshots are automatically deregistered,
        // but mio will attempt to deregister them manually.
        self.port.cancel(&*handle, token.0 as u64)
            .map_err(status_to_io_err)
            .or_else(|e| if e.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            })
    }

    pub ( in sys::fuchsia) fn register_handle<H>(&self,
                                                 handle: &H,
                                                 token: Token,
                                                 interests: Ready,
                                                 poll_opts: PollOpt) -> io::Result<()>
        where H: magenta::HandleBase
    {
        if poll_opts.is_level() && !poll_opts.is_oneshot() {
            panic!("Repeated level-triggered events are not supported on Fuchsia handles.");
        }

        handle.wait_async(&self.port,
                          key_from_token_and_type(token, RegType::Handle),
                          ready_to_signals(interests),
                          poll_opts_to_wait_async(poll_opts))
              .map_err(status_to_io_err)
    }


    pub ( in sys::fuchsia) fn deregister_handle<H>(&self,
                                                   handle: &H,
                                                   token: Token) -> io::Result<()>
        where H: magenta::HandleBase
    {
        self.port.cancel(handle,
                         key_from_token_and_type(token, RegType::Handle))
                 .map_err(status_to_io_err)
    }
}

fn ready_to_signals(ready: Ready) -> magenta::Signals {
    // Get empty notifications for peer closing or other miscellaneous signals:
    magenta_sys::MX_OBJECT_PEER_CLOSED |
    magenta::MX_EVENT_SIGNALED |

    if ready.is_writable() {
        magenta_sys::MX_OBJECT_WRITABLE
    } else {
        magenta::MX_SIGNAL_NONE
    } |

    if ready.is_readable() {
        magenta_sys::MX_OBJECT_READABLE
    } else {
        magenta::MX_SIGNAL_NONE
    }
}

fn signals_to_ready(signals: magenta::Signals) -> Ready {
    (if signals.contains(magenta_sys::MX_OBJECT_WRITABLE) {
        Ready::writable()
    } else {
        Ready::empty()
    }) |

    (if signals.contains(magenta_sys::MX_OBJECT_READABLE) {
        Ready::readable()
    } else {
        Ready::empty()
    })
}

pub struct Events {
    /// The Fuchsia selector only handles one event at a time, so there's no reason to
    /// provide storage for multiple events.
    event_opt: Option<Event>
}

impl Events {
    pub fn with_capacity(_u: usize) -> Events { Events { event_opt: None } }
    pub fn len(&self) -> usize {
        if self.event_opt.is_some() { 1 } else { 0 }
    }
    pub fn capacity(&self) -> usize {
        1
    }
    pub fn is_empty(&self) -> bool {
        self.event_opt.is_none()
    }
    pub fn get(&self, idx: usize) -> Option<Event> {
        if idx == 0 { self.event_opt } else { None }
    }
    pub fn push_event(&mut self, event: Event) {
        assert!(::std::mem::replace(&mut self.event_opt, Some(event)).is_none(),
        "Only one event at a time can be pushed to Fuchsia `Events`.");
    }
}

impl fmt::Debug for Events {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Events {{ len: {} }}", self.len())
    }
}
