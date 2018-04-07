use {Register, Token};
use std::{fmt, io};

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
///             set_readiness.set_readiness(Ready::readable());
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

bitflags! {
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
    pub struct PollOpt: usize {
        /// `PollOpt` representing edge-triggered notifications.
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
        const EDGE    = 0b0001;
        /// `PollOpt` representing level-triggered notifications.
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
        const LEVEL   = 0b0010;
        /// `PollOpt` representing oneshot notifications.
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
        const ONESHOT = 0b0100;
    }
}

impl PollOpt {
    #[deprecated(since = "0.7.0", note = "use PollOpt::EDGE instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn edge() -> PollOpt {
        Self::EDGE
    }

    #[deprecated(since = "0.7.0", note = "use PollOpt::LEVEL instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn level() -> PollOpt {
        Self::LEVEL
    }

    #[deprecated(since = "0.7.0", note = "use PollOpt::ONESHOT instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn oneshot() -> PollOpt {
        Self::ONESHOT
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
        self.contains(Self::EDGE)
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
        self.contains(Self::LEVEL)
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
        self.contains(Self::ONESHOT)
    }
}

impl fmt::Display for PollOpt {
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
    assert_eq!("(empty)", format!("{}", PollOpt::empty()));
    assert_eq!("Edge-Triggered", format!("{}", PollOpt::EDGE));
    assert_eq!("Level-Triggered", format!("{}", PollOpt::LEVEL));
    assert_eq!("OneShot", format!("{}", PollOpt::ONESHOT));
}

bitflags! {
    /// A set of readiness event kinds
    ///
    /// `Ready` is a set of operation descriptors indicating which kind of an
    /// operation is ready to be performed. For example, `Ready::readable()`
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
    /// For high level documentation on polling and readiness, see [`Poll`].
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::readable() | Ready::writable();
    ///
    /// assert!(ready.is_readable());
    /// assert!(ready.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    /// [`readable`]: #method.readable
    /// [`writable`]: #method.writable
    /// [readiness]: struct.Poll.html#readiness-operations
    pub struct Ready: usize {
        /// `Ready` representing readable readiness.
        ///
        /// See [`Poll`] for more documentation on polling.
        ///
        /// # Examples
        ///
        /// ```
        /// use mio::Ready;
        ///
        /// let ready = Ready::readable();
        ///
        /// assert!(ready.is_readable());
        /// ```
        ///
        /// [`Poll`]: struct.Poll.html
        const READABLE = 0b00001;
        /// `Ready` representing writable readiness.
        ///
        /// See [`Poll`] for more documentation on polling.
        ///
        /// # Examples
        ///
        /// ```
        /// use mio::Ready;
        ///
        /// let ready = Ready::writable();
        ///
        /// assert!(ready.is_writable());
        /// ```
        ///
        /// [`Poll`]: struct.Poll.html
        #[inline]
        const WRITABLE = 0b00010;
    }
}


impl Ready {
    #[doc(hidden)]
    #[inline]
    pub fn new(bits: usize) -> Self {
        Self { bits }
    }

    #[deprecated(since = "0.7.0", note = "use Ready::READABLE instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn readable() -> Ready {
        Self::READABLE
    }

    #[deprecated(since = "0.7.0", note = "use Ready::WRITABLE instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn writable() -> Ready {
        Self::WRITABLE
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
    /// let ready = Ready::readable();
    ///
    /// assert!(ready.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.contains(Self::READABLE)
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
    /// let ready = Ready::writable();
    ///
    /// assert!(ready.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_writable(&self) -> bool {
        self.contains(Self::WRITABLE)
    }
}

impl fmt::Display for Ready {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (Ready::readable(), "Readable"),
            (Ready::writable(), "Writable")];

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
    assert_eq!("(empty)", format!("{}", Ready::empty()));
    assert_eq!("Readable", format!("{}", Ready::readable()));
    assert_eq!("Writable", format!("{}", Ready::writable()));
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
/// let event = Event::new(Ready::readable() | Ready::writable(), Token(0));
///
/// assert_eq!(event.readiness(), Ready::readable() | Ready::writable());
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
    /// let event = Event::new(Ready::readable() | Ready::writable(), Token(0));
    ///
    /// assert_eq!(event.readiness(), Ready::readable() | Ready::writable());
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
    /// let event = Event::new(Ready::readable() | Ready::writable(), Token(0));
    ///
    /// assert_eq!(event.readiness(), Ready::readable() | Ready::writable());
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
    /// let event = Event::new(Ready::readable() | Ready::writable(), Token(0));
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

// Used internally to mutate an `Event` in place
// Not used on all platforms
#[allow(dead_code)]
pub fn kind_mut(event: &mut Event) -> &mut Ready {
    &mut event.kind
}
