use std::{fmt, io, ops};

use crate::{Ready, Interests, Registry, Token};

/// A value that may be registered with `Registry`
///
/// Values that implement `Evented` can be registered with `Registry`. Users of
/// Mio should not use the `Evented` trait functions directly. Instead, the
/// equivalent functions on `Registry` should be used.
///
/// See [`Registry`] for more details.
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
/// [`Registry`]: ../struct.Registry.html
/// [`Registration`]: ../struct.Registration.html
/// [`SetReadiness`]: ../struct.SetReadiness.html
///
/// # Examples
///
/// Implementing `Evented` on a struct containing a socket:
///
/// ```
/// use mio::{Interests, Registry, PollOpt, Token};
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
///     fn register(&self, registry: &Registry, token: Token, interests: Interests, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `register` call to `socket`
///         self.socket.register(registry, token, interests, opts)
///     }
///
///     fn reregister(&self, registry: &Registry, token: Token, interests: Interests, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `reregister` call to `socket`
///         self.socket.reregister(registry, token, interests, opts)
///     }
///
///     fn deregister(&self, registry: &Registry) -> io::Result<()> {
///         // Delegate the `deregister` call to `socket`
///         self.socket.deregister(registry)
///     }
/// }
/// ```
///
/// Implement `Evented` using [`Registration`] and [`SetReadiness`].
///
/// ```
/// use mio::{Ready, Interests, Registration, Registry, PollOpt, Token};
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
///     fn register(&self, registry: &Registry, token: Token, interests: Interests, opts: PollOpt)
///         -> io::Result<()>
///     {
///         self.registration.register(registry, token, interests, opts)
///     }
///
///     fn reregister(&self, registry: &Registry, token: Token, interests: Interests, opts: PollOpt)
///         -> io::Result<()>
///     {
///         self.registration.reregister(registry, token, interests, opts)
///     }
///
///     fn deregister(&self, registry: &Registry) -> io::Result<()> {
///         self.registration.deregister(registry)
///     }
/// }
/// ```
pub trait Evented {
    /// Register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use [`Registry::register`]
    /// instead. Implementors should handle registration by either delegating
    /// the call to another `Evented` type or creating a [`Registration`].
    ///
    /// [`Registry::register`]: ../struct.Registry.html#method.register
    /// [`Registration`]: ../struct.Registration.html
    fn register(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()>;

    /// Re-register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use [`Registry::reregister`]
    /// instead. Implementors should handle re-registration by either delegating
    /// the call to another `Evented` type or calling
    /// [`SetReadiness::set_readiness`].
    ///
    /// [`Registry::reregister`]: ../struct.Registry.html#method.reregister
    /// [`SetReadiness::set_readiness`]: ../struct.SetReadiness.html#method.set_readiness
    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()>;

    /// Deregister `self` from the given `Registry` instance
    ///
    /// This function should not be called directly. Use [`Registry::deregister`]
    /// instead. Implementors should handle deregistration by either delegating
    /// the call to another `Evented` type or by dropping the [`Registration`]
    /// associated with `self`.
    ///
    /// [`Registry::deregister`]: ../struct.Registry.html#method.deregister
    /// [`Registration`]: ../struct.Registration.html
    fn deregister(&self, registry: &Registry) -> io::Result<()>;
}

impl Evented for Box<dyn Evented> {
    fn register(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().register(registry, token, interests, opts)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests, opts)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

impl<T: Evented> Evented for Box<T> {
    fn register(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().register(registry, token, interests, opts)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests, opts)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

impl<T: Evented> Evented for ::std::sync::Arc<T> {
    fn register(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().register(registry, token, interests, opts)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests, opts)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

/// Options supplied when registering an `Evented` handle with `Poll`
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
/// let opts = PollOpt::edge() | PollOpt::oneshot();
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
    /// let opt = PollOpt::empty();
    ///
    /// assert!(!opt.is_level());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn empty() -> PollOpt {
        PollOpt(0)
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
    /// let opt = PollOpt::edge();
    ///
    /// assert!(opt.is_edge());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn edge() -> PollOpt {
        PollOpt(0b0001)
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
    /// let opt = PollOpt::level();
    ///
    /// assert!(opt.is_level());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn level() -> PollOpt {
        PollOpt(0b0010)
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
    /// let opt = PollOpt::oneshot();
    ///
    /// assert!(opt.is_oneshot());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn oneshot() -> PollOpt {
        PollOpt(0b0100)
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
    /// let opt = PollOpt::edge();
    ///
    /// assert!(opt.is_edge());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_edge(&self) -> bool {
        self.contains(PollOpt::edge())
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
    /// let opt = PollOpt::level();
    ///
    /// assert!(opt.is_level());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_level(&self) -> bool {
        self.contains(PollOpt::level())
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
    /// let opt = PollOpt::oneshot();
    ///
    /// assert!(opt.is_oneshot());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_oneshot(&self) -> bool {
        self.contains(PollOpt::oneshot())
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
    /// let opt = PollOpt::oneshot();
    ///
    /// assert!(opt.contains(PollOpt::oneshot()));
    /// assert!(!opt.contains(PollOpt::edge()));
    /// ```
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::oneshot() | PollOpt::edge();
    ///
    /// assert!(opt.contains(PollOpt::oneshot()));
    /// assert!(opt.contains(PollOpt::edge()));
    /// ```
    ///
    /// ```
    /// use mio::PollOpt;
    ///
    /// let opt = PollOpt::oneshot() | PollOpt::edge();
    ///
    /// assert!(!PollOpt::oneshot().contains(opt));
    /// assert!(opt.contains(opt));
    /// assert!((opt | PollOpt::level()).contains(opt));
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
    /// let mut opt = PollOpt::empty();
    /// opt.insert(PollOpt::oneshot());
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
    /// let mut opt = PollOpt::oneshot();
    /// opt.remove(PollOpt::oneshot());
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
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut one = false;
        let flags = [
            (PollOpt::edge(), "Edge-Triggered"),
            (PollOpt::level(), "Level-Triggered"),
            (PollOpt::oneshot(), "OneShot"),
        ];

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one {
                    write!(fmt, " | ")?
                }
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
    assert_eq!("(empty)", format!("{:?}", PollOpt::empty()));
    assert_eq!("Edge-Triggered", format!("{:?}", PollOpt::edge()));
    assert_eq!("Level-Triggered", format!("{:?}", PollOpt::level()));
    assert_eq!("OneShot", format!("{:?}", PollOpt::oneshot()));
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
    token: Token,
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
            token,
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

pub fn ready_as_usize(events: Ready) -> usize {
    events.as_usize()
}

pub fn opt_as_usize(opt: PollOpt) -> usize {
    opt.0
}

pub fn ready_from_usize(events: usize) -> Ready {
    Ready::from_usize(events)
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
