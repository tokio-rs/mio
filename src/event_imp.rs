use {Register, Token};
use std::{fmt, io, ops};

/// A value that may be registered with `Register`
///
/// Values that implement `Evented` can be registered with `Register`. Users of
/// Mio should not use the `Evented` trait functions directly. Instead, the
/// equivalent functions on `Register` should be used.
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
/// [`Poll`]: ../struct.Poll.html
/// [`Registration`]: ../struct.Registration.html
/// [`SetReadiness`]: ../struct.SetReadiness.html
///
/// # Examples
///
/// Implementing `Evented` on a struct containing a socket:
///
/// ```
/// use mio::{Ready, Register, PollOpt, Token};
/// use mio::event::Evented;
/// use mio::net::TcpStream;
///
/// use std::io;
///
/// pub struct MyEvented {
///     socket: TcpStream,
/// }
///
/// impl Evented for MyEvented {
///     fn register(&self, register: &Register, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `register` call to `socket`
///         self.socket.register(register, token, interest, opts)
///     }
///
///     fn reregister(&self, register: &Register, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `reregister` call to `socket`
///         self.socket.reregister(register, token, interest, opts)
///     }
///
///     fn deregister(&self, register: &Register) -> io::Result<()> {
///         // Delegate the `deregister` call to `socket`
///         self.socket.deregister(register)
///     }
/// }
/// ```
///
/// Implement `Evented` using [`Registration`] and [`SetReadiness`].
///
/// ```
/// use mio::{Ready, Registration, Register, PollOpt, Token};
/// use mio::event::Evented;
///
/// use std::io;
/// use std::time::Instant;
/// use std::thread;
///
/// pub struct Deadline {
///     when: Instant,
///     registration: Registration,
/// }
///
/// impl Deadline {
///     pub fn new(when: Instant) -> Deadline {
///         let (registration, set_readiness) = Registration::new();
///
///         thread::spawn(move || {
///             let now = Instant::now();
///
///             if now < when {
///                 thread::sleep(when - now);
///             }
///
///             set_readiness.set_readiness(Ready::READABLE);
///         });
///
///         Deadline {
///             when: when,
///             registration: registration,
///         }
///     }
///
///     pub fn is_elapsed(&self) -> bool {
///         Instant::now() >= self.when
///     }
/// }
///
/// impl Evented for Deadline {
///     fn register(&self, register: &Register, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         self.registration.register(register, token, interest, opts)
///     }
///
///     fn reregister(&self, register: &Register, token: Token, interest: Ready, opts: PollOpt)
///         -> io::Result<()>
///     {
///         self.registration.reregister(register, token, interest, opts)
///     }
///
///     fn deregister(&self, register: &Register) -> io::Result<()> {
///         self.registration.deregister(register)
///     }
/// }
/// ```
pub trait Evented {
    /// Register `self` with the given `Register` instance.
    ///
    /// This function should not be called directly. Use [`Register::register`]
    /// instead. Implementors should handle registration by either delegating
    /// the call to another `Evented` type or creating a [`Registration`].
    ///
    /// [`Register::register`]: ../struct.Register.html#method.register
    /// [`Registration`]: ../struct.Registration.html
    fn register(&self, register: &Register, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>;

    /// Re-register `self` with the given `Register` instance.
    ///
    /// This function should not be called directly. Use [`Register::reregister`]
    /// instead. Implementors should handle re-registration by either delegating
    /// the call to another `Evented` type or calling
    /// [`SetReadiness::set_readiness`].
    ///
    /// [`Register::reregister`]: ../struct.Register.html#method.reregister
    /// [`SetReadiness::set_readiness`]: ../struct.SetReadiness.html#method.set_readiness
    fn reregister(&self, register: &Register, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()>;

    /// Deregister `self` from the given `Register` instance
    ///
    /// This function should not be called directly. Use [`Register::deregister`]
    /// instead. Implementors should handle deregistration by either delegating
    /// the call to another `Evented` type or by dropping the [`Registration`]
    /// associated with `self`.
    ///
    /// [`Register::deregister`]: ../struct.Register.html#method.deregister
    /// [`Registration`]: ../struct.Registration.html
    fn deregister(&self, register: &Register) -> io::Result<()>;
}

/// Options supplied when registering an `Evented` handle with `Register`
///
/// `PollOpt` values can be combined together using the various bitwise
/// operators.
///
/// For high level documentation on polling and poll options, see [`Poll`].
///
/// # Examples
///
/// ```
/// use mio::PollOpt;
///
/// let opts = PollOpt::EDGE | PollOpt::ONESHOT;
///
/// assert!(opts.is_edge());
/// assert!(opts.is_oneshot());
/// assert!(!opts.is_level());
/// ```
///
/// [`Poll`]: struct.Poll.html
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct PollOpt(usize);

impl PollOpt {
    /// Return a `PollOpt` representing no set options.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::EMPTY;
    ///
    /// assert!(!opt.is_level());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub const EMPTY: PollOpt = PollOpt(0);

    #[deprecated(since = "0.6.10", note = "use PollOp::EMPTY instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn empty() -> PollOpt {
        PollOpt::EMPTY
    }



    /// Return a `PollOpt` representing edge-triggered notifications.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::EDGE;
    ///
    /// assert!(opt.is_edge());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub const EDGE: PollOpt = PollOpt(0b0001);

    #[deprecated(since = "0.6.10", note = "use PollOp::EDGE instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn edge() -> PollOpt {
        PollOpt::EDGE
    }

    /// Return a `PollOpt` representing level-triggered notifications.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::LEVEL;
    ///
    /// assert!(opt.is_level());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub const LEVEL: PollOpt = PollOpt(0b0010);

    #[deprecated(since = "0.6.10", note = "use PollOp::EMPTY instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn level() -> PollOpt {
        PollOpt::LEVEL
    }

    /// Return a `PollOpt` representing oneshot notifications.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::ONESHOT;
    ///
    /// assert!(opt.is_oneshot());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub const ONESHOT: PollOpt = PollOpt(0b0100);

    #[deprecated(since = "0.6.10", note = "use PollOp::ONESHOT instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn oneshot() -> PollOpt {
        PollOpt::ONESHOT
    }

    /// Returns true if the options include edge-triggered notifications.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::EDGE;
    ///
    /// assert!(opt.is_edge());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_edge(&self) -> bool {
        self.contains(PollOpt::EDGE)
    }

    /// Returns true if the options include level-triggered notifications.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::LEVEL;
    ///
    /// assert!(opt.is_level());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_level(&self) -> bool {
        self.contains(PollOpt::LEVEL)
    }

    /// Returns true if the options includes oneshot.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::ONESHOT;
    ///
    /// assert!(opt.is_oneshot());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_oneshot(&self) -> bool {
        self.contains(PollOpt::ONESHOT)
    }

    /// Returns true if `self` is a superset of `other`.
    ///
    /// `other` may represent more than one option, in which case the function
    /// only returns true if `self` contains all of the options specified in
    /// `other`.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::ONESHOT;
    ///
    /// assert!(opt.contains(PollOpt::ONESHOT));
    /// assert!(!opt.contains(PollOpt::EDGE));
    /// ```
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::ONESHOT | PollOpt::EDGE;
    ///
    /// assert!(opt.contains(PollOpt::ONESHOT));
    /// assert!(opt.contains(PollOpt::EDGE));
    /// ```
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::ONESHOT | PollOpt::EDGE;
    ///
    /// assert!(!PollOpt::ONESHOT.contains(opt));
    /// assert!(opt.contains(opt));
    /// assert!((opt | PollOpt::LEVEL).contains(opt));
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn contains(&self, other: PollOpt) -> bool {
        (*self & other) == other
    }

    /// Adds all options represented by `other` into `self`.
    ///
    /// This is equivalent to `*self = *self | other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let mut opt = PollOpt::EMPTY;
    /// opt.insert(PollOpt::ONESHOT);
    ///
    /// assert!(opt.is_oneshot());
    /// ```
    #[inline]
    pub fn insert(&mut self, other: PollOpt) {
        self.0 |= other.0;
    }

    /// Removes all options represented by `other` from `self`.
    ///
    /// This is equivalent to `*self = *self & !other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let mut opt = PollOpt::ONESHOT;
    /// opt.remove(PollOpt::ONESHOT);
    ///
    /// assert!(!opt.is_oneshot());
    /// ```
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

impl fmt::Debug for PollOpt {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (PollOpt::EDGE, "Edge-Triggered"),
            (PollOpt::LEVEL, "Level-Triggered"),
            (PollOpt::ONESHOT, "OneShot")];

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one { write!(fmt, " | ")? }
                write!(fmt, "{}", msg)?;

                one = true
            }
        }

        if !one {
            fmt.write_str("(empty)")?;
        }

        Ok(())
    }
}

#[test]
fn test_debug_pollopt() {
    assert_eq!("(empty)", format!("{:?}", PollOpt::EMPTY));
    assert_eq!("Edge-Triggered", format!("{:?}", PollOpt::EDGE));
    assert_eq!("Level-Triggered", format!("{:?}", PollOpt::LEVEL));
    assert_eq!("OneShot", format!("{:?}", PollOpt::ONESHOT));
}

/// A set of readiness event kinds
///
/// `Ready` is a set of operation descriptors indicating which kind of an
/// operation is ready to be performed. For example, `Ready::READABLE`
/// indicates that the associated `Evented` handle is ready to perform a
/// `read` operation.
///
/// This struct only represents portable event kinds. Since only readable and
/// writable events are guaranteed to be raised on all systems, those are the
/// only ones available via the `Ready` struct. There are also platform specific
/// extensions to `Ready`, i.e. `UnixReady`, which provide additional readiness
/// event kinds only available on unix platforms.
///
/// `Ready` values can be combined together using the various bitwise operators.
///
/// For high level documentation on polling and [readiness], see [`Poll`].
///
/// # Examples
///
/// ```
/// use mio::Ready;
///
/// let ready = Ready::READABLE | Ready::WRITABLE;
///
/// assert!(ready.is_readable());
/// assert!(ready.is_writable());
/// ```
///
/// [`Poll`]: struct.Poll.html
/// [readiness]: struct.Poll.html#readiness-operations
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]

pub struct Ready(usize);

const READABLE: usize = 0b00001;
const WRITABLE: usize = 0b00010;
const ERROR: usize    = 0b00100;
const HUP: usize      = 0b01000;

impl Ready {
    /// Returns the empty `Ready` set.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::EMPTY;
    ///
    /// assert!(!ready.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub const EMPTY: Ready = Ready(0);

    /// Returns a `Ready` representing readable readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::READABLE;
    ///
    /// assert!(ready.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub const READABLE: Ready = Ready(READABLE);

    #[deprecated(since = "0.6.10", note = "use Ready::READABLE instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    pub fn readable() -> Ready {
        Ready::READABLE
    }

    /// Returns a `Ready` representing writable readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::WRITABLE;
    ///
    /// assert!(ready.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub const WRITABLE: Ready =  Ready(WRITABLE);

    #[deprecated(since = "0.6.10", note = "use Ready::WRITABLE instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    pub fn writable() -> Ready {
        Ready::WRITABLE
    }

    /// Returns true if `Ready` is the empty set
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::EMPTY;
    /// assert!(ready.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        *self == Ready::EMPTY
    }

    /// Returns true if the value includes readable readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::READABLE;
    ///
    /// assert!(ready.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.contains(Ready::READABLE)
    }

    /// Returns true if the value includes writable readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::WRITABLE;
    ///
    /// assert!(ready.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_writable(&self) -> bool {
        self.contains(Ready::WRITABLE)
    }

    /// Adds all readiness represented by `other` into `self`.
    ///
    /// This is equivalent to `*self = *self | other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let mut readiness = Ready::EMPTY;
    /// readiness.insert(Ready::READABLE);
    ///
    /// assert!(readiness.is_readable());
    /// ```
    #[inline]
    pub fn insert<T: Into<Self>>(&mut self, other: T) {
        let other = other.into();
        self.0 |= other.0;
    }

    /// Removes all options represented by `other` from `self`.
    ///
    /// This is equivalent to `*self = *self & !other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let mut readiness = Ready::READABLE;
    /// readiness.remove(Ready::READABLE);
    ///
    /// assert!(!readiness.is_readable());
    /// ```
    #[inline]
    pub fn remove<T: Into<Self>>(&mut self, other: T) {
        let other = other.into();
        self.0 &= !other.0;
    }

    /// Returns true if `self` is a superset of `other`.
    ///
    /// `other` may represent more than one readiness operations, in which case
    /// the function only returns true if `self` contains all readiness
    /// specified in `other`.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let readiness = Ready::READABLE;
    ///
    /// assert!(readiness.contains(Ready::READABLE));
    /// assert!(!readiness.contains(Ready::WRITABLE));
    /// ```
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let readiness = Ready::READABLE | Ready::WRITABLE;
    ///
    /// assert!(readiness.contains(Ready::READABLE));
    /// assert!(readiness.contains(Ready::WRITABLE));
    /// ```
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let readiness = Ready::READABLE | Ready::WRITABLE;
    ///
    /// assert!(!Ready::READABLE.contains(readiness));
    /// assert!(readiness.contains(readiness));
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn contains<T: Into<Self>>(&self, other: T) -> bool {
        let other = other.into();
        (*self & other) == other
    }
}

impl<T: Into<Ready>> ops::BitOr<T> for Ready {
    type Output = Ready;

    #[inline]
    fn bitor(self, other: T) -> Ready {
        Ready(self.0 | other.into().0)
    }
}

impl<T: Into<Ready>> ops::BitOrAssign<T> for Ready {
    #[inline]
    fn bitor_assign(&mut self, other: T) {
        self.0 |= other.into().0;
    }
}

impl<T: Into<Ready>> ops::BitXor<T> for Ready {
    type Output = Ready;

    #[inline]
    fn bitxor(self, other: T) -> Ready {
        Ready(self.0 ^ other.into().0)
    }
}

impl<T: Into<Ready>> ops::BitXorAssign<T> for Ready {
    #[inline]
    fn bitxor_assign(&mut self, other: T) {
        self.0 ^= other.into().0;
    }
}

impl<T: Into<Ready>> ops::BitAnd<T> for Ready {
    type Output = Ready;

    #[inline]
    fn bitand(self, other: T) -> Ready {
        Ready(self.0 & other.into().0)
    }
}

impl<T: Into<Ready>> ops::BitAndAssign<T> for Ready {
    #[inline]
    fn bitand_assign(&mut self, other: T) {
        self.0 &= other.into().0
    }
}

impl<T: Into<Ready>> ops::Sub<T> for Ready {
    type Output = Ready;

    #[inline]
    fn sub(self, other: T) -> Ready {
        Ready(self.0 & !other.into().0)
    }
}

impl<T: Into<Ready>> ops::SubAssign<T> for Ready {
    #[inline]
    fn sub_assign(&mut self, other: T) {
        self.0 &= !other.into().0;
    }
}

impl fmt::Debug for Ready {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (Ready::READABLE, "Readable"),
            (Ready::WRITABLE, "Writable"),
            (Ready(ERROR), "Error"),
            (Ready(HUP), "Hup")];

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one { write!(fmt, " | ")? }
                write!(fmt, "{}", msg)?;

                one = true
            }
        }

        if !one {
            fmt.write_str("(empty)")?;
        }

        Ok(())
    }
}

#[test]
fn test_debug_ready() {
    assert_eq!("(empty)", format!("{:?}", Ready::EMPTY));
    assert_eq!("Readable", format!("{:?}", Ready::READABLE));
    assert_eq!("Writable", format!("{:?}", Ready::WRITABLE));
}

/// An readiness event returned by [`Poll::poll`].
///
/// `Event` is a [readiness state] paired with a [`Token`]. It is returned by
/// [`Poll::poll`].
///
/// For more documentation on polling and events, see [`Poll`].
///
/// # Examples
///
/// ```
/// use mio::{Ready, Token};
/// use mio::event::Event;
///
/// let event = Event::new(Ready::READABLE | Ready::WRITABLE, Token(0));
///
/// assert_eq!(event.readiness(), Ready::READABLE | Ready::WRITABLE);
/// assert_eq!(event.token(), Token(0));
/// ```
///
/// [`Poll::poll`]: ../struct.Poll.html#method.poll
/// [`Poll`]: ../struct.Poll.html
/// [readiness state]: ../struct.Ready.html
/// [`Token`]: ../struct.Token.html
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Event {
    kind: Ready,
    token: Token
}

impl Event {
    /// Creates a new `Event` containing `readiness` and `token`
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::{Ready, Token};
    /// use mio::event::Event;
    ///
    /// let event = Event::new(Ready::READABLE, Token(0));
    ///
    /// assert_eq!(event.readiness(), Ready::READABLE);
    /// assert_eq!(event.token(), Token(0));
    /// ```
    pub fn new(readiness: Ready, token: Token) -> Event {
        Event {
            kind: readiness,
            token: token,
        }
    }

    /// Returns the event's readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::{Ready, Token};
    /// use mio::event::Event;
    ///
    /// let event = Event::new(Ready::READABLE, Token(0));
    ///
    /// assert_eq!(event.readiness(), Ready::READABLE);
    /// ```
    pub fn readiness(&self) -> Ready {
        self.kind
    }

    /// Returns the event's token.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::{Ready, Token};
    /// use mio::event::Event;
    ///
    /// let event = Event::new(Ready::READABLE, Token(0));
    ///
    /// assert_eq!(event.token(), Token(0));
    /// ```
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
