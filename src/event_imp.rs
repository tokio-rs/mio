use {Poll, Token};
use std::{fmt, io, ops};

/// A value that may be registered with `Poll`
///
/// Values that implement `Evented` can be registered with `Poll`. Users of Mio
/// should not use the `Evented` trait functions directly. Instead, the
/// equivalent functions on `Poll` should be used.
///
/// See [`Poll`] for more details.
///
/// # Implementing `Evented`
///
/// There are two types of `Evented` values.
///
/// * **System** handles, which are backed by sockets or other system handles.
/// These `Evented` handles will be monitored by the system selector. In this
/// case, an implementation of `Evented` delegates to a lower level handle.
///
/// * **User** handles, which are driven entirely in user space using
/// [`Registration`] and [`SetReadiness`]. In this case, the implementer takes
/// responsibility for driving the readiness state changes.
///
/// # Examples
///
/// Implementing `Evented` on a struct containing a socket:
///
/// ```
/// use mio::{Ready, Poll, PollOpt, Token};
/// use mio::event::Evented;
/// use mio::tcp::TcpStream;
///
/// use std::io;
///
/// pub struct MyEvented {
///     socket: TcpStream,
/// }
///
/// impl Evented for MyEvented {
///     fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `register` call to `socket`
///         self.socket.register(poll, token, interest, opts)
///     }
///
///     fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `reregister` call to `socket`
///         self.socket.reregister(poll, token, interest, opts)
///     }
///
///     fn deregister(&self, poll: &Poll) -> io::Result<()> {
///         // Delegate the `deregister` call to `socket`
///         self.socket.deregister(poll)
///     }
/// }
/// ```
///
/// Implement `Evented` using [`Registration`] and [`SetReadiness`].
///
/// ```
/// use mio::{Ready, Registration, Poll, PollOpt, Token};
/// use mio::event::Evented;
///
/// use std::io;
/// use std::sync::Mutex;
/// use std::time::Instant;
/// use std::thread;
///
/// pub struct Deadline {
///     when: Instant,
///     registration: Mutex<Option<Registration>>,
/// }
///
/// impl Deadline {
///     pub fn is_elapsed(&self) -> bool {
///         Instant::now() >= self.when
///     }
/// }
///
/// impl Evented for Deadline {
///     fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         let mut registration = self.registration.lock().unwrap();
///
///         if registration.is_some() {
///             return Err(io::Error::new(io::ErrorKind::Other, "already registered"));
///         }
///
///         let (r, set_readiness) = Registration::new(poll, token, interest, opts);
///         *registration = Some(r);
///
///         let when = self.when;
///
///         thread::spawn(move || {
///             let now = Instant::now();
///
///             if now < when {
///                 thread::sleep(when - now);
///             }
///
///             set_readiness.set_readiness(Ready::readable());
///         });
///
///         Ok(())
///     }
///
///     fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         match *self.registration.lock().unwrap() {
///             Some(ref registration) => registration.update(token, interest, opts),
///             None => Err(io::Error::new(io::ErrorKind::Other, "not registered")),
///         }
///     }
///
///     fn deregister(&self, poll: &Poll) -> io::Result<()> {
///         let _ = self.registration.lock().unwrap().take();
///     }
/// }
/// ```
///
/// [`Poll`]: struct.Poll.html
/// [`Registration`]: struct.Registration.html
/// [`SetReadiness`]: struct.SetReadiness.html
pub trait Evented {
    /// Register `self` with the given `Poll` instance.
    ///
    /// This function should not be called directly. Use [`Poll::register`]
    /// instead.
    ///
    /// Implementors should handle registration by either delegating the call to
    /// another `Evented` type or creating a [`Registration`].
    ///
    /// See [struct] documentation for more details.
    ///
    /// [`Poll::register`]: struct.Poll.html#method.register
    /// [`Registration`]: struct.Registration.html
    /// [struct]: #
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>;

    /// Re-register `self` with the given `Poll` instance.
    ///
    /// This function should not be called directly. Use [`Poll::reregister`]
    /// instead.
    ///
    /// Implementors should handle re-registration by either delegating the call to
    /// another `Evented` type or calling [`Registration::update`].
    ///
    /// See [struct] documentation for more details.
    ///
    /// [`Poll::reregister`]: struct.Poll.html#method.register
    /// [`Registration::update`]: struct.Registration.html#method.update
    /// [struct]: #
    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>;

    /// Deregister `self` from the given `Poll` instance
    ///
    /// This function should not be called directly. Use [`Poll::deregister`]
    /// instead.
    ///
    /// Implementors shuld handle deregistration by either delegating the call
    /// to another `Evented` type or by dropping the [`Registration`] associated
    /// with `self`.
    ///
    /// See [struct] documentation for more details.
    ///
    /// [`Poll::deregister`]: struct.Poll.html#method.deregister
    /// [`Registration`]: struct.Registration.html
    /// [struct]: #
    fn deregister(&self, poll: &Poll) -> io::Result<()>;
}

/// Configures readiness polling behavior for a given `Evented` value.
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct PollOpt(usize);

impl PollOpt {
    #[inline]
    pub fn empty() -> PollOpt {
        PollOpt(0)
    }

    #[inline]
    pub fn edge() -> PollOpt {
        PollOpt(0b0001)
    }

    #[inline]
    pub fn level() -> PollOpt {
        PollOpt(0b0010)
    }

    #[inline]
    pub fn oneshot() -> PollOpt {
        PollOpt(0b0100)
    }

    #[inline]
    pub fn urgent() -> PollOpt {
        PollOpt(0b1000)
    }

    #[deprecated(since = "0.6.5", note = "removed")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn all() -> PollOpt {
        PollOpt::edge() | PollOpt::level() | PollOpt::oneshot()
    }

    #[inline]
    pub fn is_edge(&self) -> bool {
        self.contains(PollOpt::edge())
    }

    #[inline]
    pub fn is_level(&self) -> bool {
        self.contains(PollOpt::level())
    }

    #[inline]
    pub fn is_oneshot(&self) -> bool {
        self.contains(PollOpt::oneshot())
    }

    #[inline]
    pub fn is_urgent(&self) -> bool {
        self.contains(PollOpt::urgent())
    }

    #[deprecated(since = "0.6.5", note = "removed")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn bits(&self) -> usize {
        self.0
    }

    #[inline]
    pub fn contains(&self, other: PollOpt) -> bool {
        (*self & other) == other
    }

    #[inline]
    pub fn insert(&mut self, other: PollOpt) {
        self.0 |= other.0;
    }

    #[inline]
    pub fn remove(&mut self, other: PollOpt) {
        self.0 &= !other.0;
    }
}

impl ops::BitOr for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn bitor(self, other: PollOpt) -> PollOpt {
        PollOpt(self.0 | other.0)
    }
}

impl ops::BitXor for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn bitxor(self, other: PollOpt) -> PollOpt {
        PollOpt(self.0 ^ other.0)
    }
}

impl ops::BitAnd for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn bitand(self, other: PollOpt) -> PollOpt {
        PollOpt(self.0 & other.0)
    }
}

impl ops::Sub for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn sub(self, other: PollOpt) -> PollOpt {
        PollOpt(self.0 & !other.0)
    }
}

impl ops::Not for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn not(self) -> PollOpt {
        PollOpt(!self.0)
    }
}

impl fmt::Debug for PollOpt {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (PollOpt::edge(), "Edge-Triggered"),
            (PollOpt::level(), "Level-Triggered"),
            (PollOpt::oneshot(), "OneShot")];

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one { try!(write!(fmt, " | ")) }
                try!(write!(fmt, "{}", msg));

                one = true
            }
        }

        Ok(())
    }
}

/// A set of readiness events returned by `Poll`.
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Ready(usize);

const READABLE: usize = 0b0001;
const WRITABLE: usize = 0b0010;
const ERROR: usize    = 0b0100;
const HUP: usize      = 0b1000;
const READY_ALL: usize = READABLE | WRITABLE | ERROR | HUP;

pub trait ReadyUnix {
    fn error() -> Self;

    fn hup() -> Self;

    fn is_error(&self) -> bool;

    #[inline]
    fn is_hup(&self) -> bool;
}

impl Ready {
    pub fn none() -> Ready {
        Ready(0)
    }

    #[inline]
    pub fn readable() -> Ready {
        Ready(READABLE)
    }

    #[inline]
    pub fn writable() -> Ready {
        Ready(WRITABLE)
    }

    #[deprecated(since = "0.6.5", note = "use unix::ReadyExt instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn error() -> Ready {
        Ready(ERROR)
    }

    #[deprecated(since = "0.6.5", note = "use unix::ReadyExt instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn hup() -> Ready {
        Ready(HUP)
    }

    #[inline]
    pub fn all() -> Ready {
        Ready::readable() |
            Ready::writable()
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        *self == Ready::none()
    }

    #[inline]
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    #[inline]
    pub fn is_readable(&self) -> bool {
        self.contains(Ready::readable())
    }

    #[inline]
    pub fn is_writable(&self) -> bool {
        self.contains(Ready::writable())
    }

    #[deprecated(since = "0.6.5", note = "use unix::ReadyExt instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn is_error(&self) -> bool {
        self.contains(Ready(ERROR))
    }

    #[deprecated(since = "0.6.5", note = "use unix::ReadyExt instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn is_hup(&self) -> bool {
        self.contains(Ready(HUP))
    }

    #[inline]
    pub fn insert(&mut self, other: Ready) {
        self.0 |= other.0;
    }

    #[inline]
    pub fn remove(&mut self, other: Ready) {
        self.0 &= !other.0;
    }

    #[deprecated(since = "0.6.5", note = "removed")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn bits(&self) -> usize {
        self.0
    }

    #[inline]
    pub fn contains(&self, other: Ready) -> bool {
        (*self & other) == other
    }
}

impl ReadyUnix for Ready {
    #[inline]
    fn error() -> Self {
        Ready(ERROR)
    }

    #[inline]
    fn hup() -> Self {
        Ready(HUP)
    }

    #[inline]
    fn is_error(&self) -> bool {
        self.contains(Ready(ERROR))
    }

    #[inline]
    fn is_hup(&self) -> bool {
        self.contains(Ready(HUP))
    }
}

impl ops::BitOr for Ready {
    type Output = Ready;

    #[inline]
    fn bitor(self, other: Ready) -> Ready {
        Ready(self.0 | other.0)
    }
}

impl ops::BitXor for Ready {
    type Output = Ready;

    #[inline]
    fn bitxor(self, other: Ready) -> Ready {
        Ready(self.0 ^ other.0)
    }
}

impl ops::BitAnd for Ready {
    type Output = Ready;

    #[inline]
    fn bitand(self, other: Ready) -> Ready {
        Ready(self.0 & other.0)
    }
}

impl ops::Sub for Ready {
    type Output = Ready;

    #[inline]
    fn sub(self, other: Ready) -> Ready {
        Ready(self.0 & !other.0)
    }
}

impl ops::Not for Ready {
    type Output = Ready;

    #[inline]
    fn not(self) -> Ready {
        Ready(!self.0 & READY_ALL)
    }
}

impl fmt::Debug for Ready {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (Ready::readable(), "Readable"),
            (Ready::writable(), "Writable"),
            (Ready(ERROR), "Error"),
            (Ready(HUP), "Hup")];

        try!(write!(fmt, "Ready {{"));

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one { try!(write!(fmt, " | ")) }
                try!(write!(fmt, "{}", msg));

                one = true
            }
        }

        try!(write!(fmt, "}}"));

        Ok(())
    }
}

/// An readiness event returned by `Poll`.
///
/// Event represents the raw event that the OS-specific selector
/// returned. An event can represent more than one kind (such as
/// readable or writable) at a time.
///
/// These Event objects are created by the OS-specific concrete
/// Selector when they have events to report.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Event {
    kind: Ready,
    token: Token
}

impl Event {
    /// Create a new Event.
    pub fn new(kind: Ready, token: Token) -> Event {
        Event {
            kind: kind,
            token: token,
        }
    }

    pub fn readiness(&self) -> Ready {
        self.kind
    }

    #[deprecated(since = "0.6.5", note = "use Event::readiness()")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    pub fn kind(&self) -> Ready {
        self.kind
    }

    pub fn token(&self) -> Token {
        self.token
    }
}

/*
 *
 * ===== Mio internal helpers =====
 *
 */

pub fn ready_as_usize(events: Ready) -> usize {
    events.0
}

pub fn opt_as_usize(opt: PollOpt) -> usize {
    opt.0
}

pub fn ready_from_usize(events: usize) -> Ready {
    Ready(events)
}

pub fn opt_from_usize(opt: usize) -> PollOpt {
    PollOpt(opt)
}

// Used internally to mutate an `Event` in place
// Not used on all platforms
#[allow(dead_code)]
pub fn kind_mut(event: &mut Event) -> &mut Ready {
    &mut event.kind
}
