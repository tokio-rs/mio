use crate::{Registry, Token};
use std::num::NonZeroU8;
use std::{fmt, io, ops};

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

/// A set of readiness event kinds
///
/// `Ready` is a set of operation descriptors indicating which kind of an
/// operation is ready to be performed. For example, `Ready::readable()`
/// indicates that the associated `Evented` handle is ready to perform a
/// `read` operation.
///
/// `Ready` values can be combined together using the various bitwise operators.
///
/// For high level documentation on polling and readiness, see [`Poll`].
///
/// [`Poll`]: mio::Poll
///
/// # Notes
///
/// This struct represents both portable an non-portable readiness indicators.
/// Only [readable] and [writable] events are guaranteed to be raised on
/// all systems, and so those are available on all systems.
///
/// But this also provides a number of non-portable readiness indicators, such
/// as [error], [hup], [priority], [AIO] and [LIO]. These are **not** available
/// on all platforms, and can only be created on platforms that support it.
/// However it is possible to check for there presence in a set on all
/// platforms. These indicators should be treated as a hint.
///
///
/// [readable]: Ready::readable
/// [writable]: Ready::writable
/// [error]: Ready::error
/// [hup]: Ready::hup
/// [priority]: Ready::priority
/// [AIO]: Ready::aio
/// [LIO]: Ready::li0
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
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Ready(usize);

// These must be unique.
const EMPTY: usize = 0b0_000_000;
const READABLE: usize = 0b0_000_001;
const WRITABLE: usize = 0b0_000_010;
// The following are not available on all platforms.
const ERROR: usize = 0b0_000_100;
const HUP: usize = 0b0_001_000;
const PRI: usize = 0b0_010_000;
const AIO: usize = 0b0_100_000;
const LIO: usize = 0b1_000_000;

impl Ready {
    /// Returns an empty `Ready` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::empty();
    /// assert!(!ready.is_readable());
    /// ```
    pub fn empty() -> Ready {
        Ready(EMPTY)
    }

    /// Returns a `Ready` set representing readable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::readable();
    /// assert!(ready.is_readable());
    /// ```
    #[inline]
    pub fn readable() -> Ready {
        Ready(READABLE)
    }

    /// Returns a `Ready` set representing writable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::writable();
    /// assert!(ready.is_writable());
    /// ```
    #[inline]
    pub fn writable() -> Ready {
        Ready(WRITABLE)
    }

    /// Returns a `Ready` set representing error readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::error();
    /// assert!(ready.is_error());
    /// ```
    ///
    /// # Notes
    ///
    /// Only available on Unix platforms.
    #[inline]
    #[cfg(unix)]
    pub fn error() -> Ready {
        Ready(ERROR)
    }

    /// Returns a `Ready` set representing HUP readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::hup();
    /// assert!(ready.is_hup());
    /// ```
    ///
    /// # Notes
    ///
    /// Only available on Unix platforms.
    #[inline]
    #[cfg(unix)]
    pub fn hup() -> Ready {
        Ready(HUP)
    }

    /// Returns a `Ready` set representing priority readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::priority();
    /// assert!(ready.is_priority());
    /// ```
    ///
    /// # Notes
    ///
    /// Only available on DragonFlyBSD, FreeBSD, iOS and macOS.
    #[inline]
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
    pub fn priority() -> Ready {
        Ready(PRI)
    }

    /// Returns a `Ready` set representing AIO completion readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::aio();
    /// assert!(ready.is_aio());
    /// ```
    ///
    /// # Notes
    ///
    /// Only available on DragonFlyBSD, FreeBSD, iOS and macOS.
    #[inline]
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    ))]
    pub fn aio() -> Ready {
        Ready(AIO)
    }

    /// Returns a `Ready` set representing LIO completion readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::lio();
    /// assert!(ready.is_lio());
    /// ```
    ///
    /// # Notes
    ///
    /// Only available on FreeBSD.
    #[inline]
    #[cfg(any(target_os = "freebsd"))]
    pub fn lio() -> Ready {
        Ready(LIO)
    }

    /// Returns true if the `Ready` set is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::empty();
    /// assert!(ready.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0 == EMPTY
    }

    /// Returns true if the `Ready` set contains readable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::readable();
    /// assert!(ready.is_readable());
    /// ```
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.contains(Ready::readable())
    }

    /// Returns true if the `Ready` set contains writable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::writable();
    /// assert!(ready.is_writable());
    /// ```
    #[inline]
    pub fn is_writable(&self) -> bool {
        self.contains(Ready::writable())
    }

    /// Returns true if the `Ready` set contains error readiness.
    ///
    /// Error events occur when the socket enters an error state. In this case,
    /// the socket will also receive a readable or writable event. Reading or
    /// writing to the socket will result in an error.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(unix)]
    /// # {
    /// use mio::Ready;
    ///
    /// let ready = Ready::error();
    /// assert!(ready.is_error());
    /// # }
    /// ```
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_error(&self) -> bool {
        self.contains(Ready::error())
    }

    /// Returns true if the `Ready` set contains HUP readiness.
    ///
    /// HUP events occur when the remote end of a socket hangs up. In the TCP
    /// case, this occurs when the remote end of a TCP socket shuts down writes.
    ///
    /// It is also unclear if HUP readiness will remain in 0.7. See
    /// [here](https://github.com/tokio-rs/mio/issues/941)
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(unix)]
    /// # {
    /// use mio::Ready;
    ///
    /// let ready = Ready::hup();
    /// assert!(ready.is_hup());
    /// # }
    /// ```
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_hup(&self) -> bool {
        self.contains(Ready(HUP))
    }

    /// Returns true if the `Ready` set contains priority readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
    /// # {
    /// use mio::Ready;
    ///
    /// let ready = Ready::priority();
    /// assert!(ready.is_priority());
    /// # }
    /// ```
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_priority(&self) -> bool {
        self.contains(Ready(PRI))
    }

    /// Returns true if the `Ready` set contains AIO readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(any( target_os = "dragonfly", target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    /// # {
    /// use mio::Ready;
    ///
    /// let ready = Ready::aio();
    /// assert!(ready.is_aio());
    /// # }
    /// ```
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_aio(&self) -> bool {
        self.contains(Ready(AIO))
    }

    /// Returns true if the `Ready` set contains LIO readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(any(target_os = "freebsd"))]
    /// # {
    /// use mio::Ready;
    ///
    /// let ready = Ready::lio();
    /// assert!(ready.is_lio());
    /// # }
    /// ```
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_lio(&self) -> bool {
        self.contains(Ready(LIO))
    }

    /// Adds all readiness in the `other` set into `self`.
    ///
    /// This is equivalent to `*self = *self | other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let mut readiness = Ready::empty();
    /// readiness.insert(Ready::readable());
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
    /// let mut readiness = Ready::readable();
    /// readiness.remove(Ready::readable());
    /// assert!(!readiness.is_readable());
    /// ```
    #[inline]
    pub fn remove<T: Into<Self>>(&mut self, other: T) {
        let other = other.into();
        self.0 &= !other.0;
    }

    /// Returns true if `self` is a superset of `other`.
    ///
    /// The `other` set may represent more than one readiness operations, in
    /// which case the function only returns true if `self` contains **all**
    /// readiness specified in `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let readiness = Ready::readable();
    /// assert!(readiness.contains(Ready::readable()));
    /// assert!(!readiness.contains(Ready::writable()));
    /// ```
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let readiness = Ready::readable() | Ready::writable();
    ///
    /// assert!(readiness.contains(Ready::readable()));
    /// assert!(readiness.contains(Ready::writable()));
    /// ```
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let readiness = Ready::readable() | Ready::writable();
    /// assert!(!Ready::readable().contains(readiness));
    /// assert!(readiness.contains(readiness));
    /// ```
    #[inline]
    pub fn contains<T: Into<Self>>(&self, other: T) -> bool {
        let other = other.into();
        (self.0 & other.0) == other.0
    }

    /// Create a `Ready` instance using the given `usize` representation.
    ///
    /// The `usize` representation must have been obtained from a call to
    /// `Ready::as_usize`.
    ///
    /// The `usize` representation must be treated as opaque. There is no
    /// guaranteed correlation between the returned value and platform defined
    /// constants. Also, there is no guarantee that the `usize` representation
    /// will remain constant across patch releases of Mio.
    ///
    /// This function is mainly provided to allow the caller to loa a
    /// readiness value from an `AtomicUsize`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::readable();
    /// let ready_usize = ready.as_usize();
    /// let ready2 = Ready::from_usize(ready_usize);
    /// assert_eq!(ready, ready2);
    /// ```
    pub fn from_usize(val: usize) -> Ready {
        Ready(val)
    }

    /// Returns a `usize` representation of the `Ready` value.
    ///
    /// This `usize` representation must be treated as opaque. There is no
    /// guaranteed correlation between the returned value and platform defined
    /// constants. Also, there is no guarantee that the `usize` representation
    /// will remain constant across patch releases of Mio.
    ///
    /// This function is mainly provided to allow the caller to store a
    /// readiness value in an `AtomicUsize`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::readable();
    /// let ready_usize = ready.as_usize();
    /// let ready2 = Ready::from_usize(ready_usize);
    /// assert_eq!(ready, ready2);
    /// ```
    pub fn as_usize(&self) -> usize {
        self.0
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
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut one = false;
        let flags = [
            (Ready::readable(), "Readable"),
            (Ready::writable(), "Writable"),
            (Ready(ERROR), "Error"),
            (Ready(HUP), "Hup"),
            (Ready(PRI), "Priority"),
            (Ready(AIO), "AIO"),
            (Ready(LIO), "LIO"),
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
fn fmt_debug() {
    assert_eq!("(empty)", format!("{:?}", Ready::empty()));
    assert_eq!("Readable", format!("{:?}", Ready::readable()));
    assert_eq!("Writable", format!("{:?}", Ready::writable()));
    assert_eq!("Error", format!("{:?}", Ready::error()));
    assert_eq!("Hup", format!("{:?}", Ready(HUP)));
    assert_eq!("Priority", format!("{:?}", Ready(PRI)));
    assert_eq!("AIO", format!("{:?}", Ready(AIO)));
    assert_eq!("LIO", format!("{:?}", Ready(LIO)));
    assert_eq!(
        "Readable | Writable",
        format!("{:?}", Ready::readable() | Ready::writable())
    );
}

/* TODO(Thomas): check if this is still relevant.
#[test]
fn test_ready_all() {
    let readable = Ready::readable().as_usize();
    let writable = Ready::writable().as_usize();

    assert_eq!(
        READY_ALL | readable | writable,
        ERROR + HUP + AIO + LIO + PRI + readable + writable
    );

    // Issue #896.
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
    assert!(!Ready::from(Ready::priority()).is_writable());
}
*/

/// Interests used in registering.
///
/// Interests are used in registering [`Evented`] handles with [`Poll`],
/// they indicate what readiness should be monitored for. For example if a
/// socket is registered with readable interests and the socket becomes
/// writable, no event will be returned from [`poll`].
///
/// The size of `Option<Interests>` should be identical to itself.
///
/// ```
/// use std::mem::size_of;
/// use mio::Interests;
///
/// assert_eq!(size_of::<Option<Interests>>(), size_of::<Interests>());
/// ```
///
/// [`Poll`]: struct.Poll.html
/// [`readable`]: #method.readable
/// [`writable`]: #method.writable
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Interests(NonZeroU8);

impl Interests {
    const READABLE: u8 = 0b00001;

    const WRITABLE: u8 = 0b00010;

    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    ))]
    const AIO: u8 = 0b01_0000;

    #[cfg(any(target_os = "freebsd"))]
    const LIO: u8 = 0b10_0000;

    /// Returns `Interests` representing readable readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// let interests = Interests::readable();
    ///
    /// assert!(interests.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub const fn readable() -> Interests {
        Interests(unsafe { NonZeroU8::new_unchecked(Interests::READABLE) })
    }

    /// Returns `Interests` representing writable readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// let interests = Interests::writable();
    ///
    /// assert!(interests.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub const fn writable() -> Interests {
        Interests(unsafe { NonZeroU8::new_unchecked(Interests::WRITABLE) })
    }

    /// Returns `Interests` representing AIO completion readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// let interests = Interests::aio();
    ///
    /// assert!(interests.is_aio());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    ))]
    pub const fn aio() -> Interests {
        Interests(unsafe { NonZeroU8::new_unchecked(Interests::AIO) })
    }

    /// Returns `Interests` representing LIO completion readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// let interests = Interests::lio();
    ///
    /// assert!(interests.is_lio());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    #[cfg(any(target_os = "freebsd"))]
    pub const fn lio() -> Interests {
        Interests(unsafe { NonZeroU8::new_unchecked(Interests::LIO) })
    }

    /// Returns true if the value includes readable readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// let interests = Interests::readable();
    ///
    /// assert!(interests.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub fn is_readable(&self) -> bool {
        (self.0.get() & Interests::READABLE) != 0
    }

    /// Returns true if the value includes writable readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// let interests = Interests::writable();
    ///
    /// assert!(interests.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub fn is_writable(&self) -> bool {
        (self.0.get() & Interests::WRITABLE) != 0
    }

    /// Returns true if `Interests` contains AIO readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// let interests = Interests::aio();
    ///
    /// assert!(interests.is_aio());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    ))]
    #[inline]
    pub fn is_aio(&self) -> bool {
        (self.0.get() & Interests::AIO) != 0
    }

    /// Returns true if `Interests` contains LIO readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// let interests = Interests::lio();
    ///
    /// assert!(interests.is_lio());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    #[cfg(any(target_os = "freebsd"))]
    pub fn is_lio(&self) -> bool {
        (self.0.get() & Interests::LIO) != 0
    }

    /// Returns `Ready` contains `Interests` readiness
    ///
    /// It should and only can be used in crate, and will be deprecated in the future.
    /// So don't use it unless you have no other choice.
    pub(crate) fn to_ready(&self) -> Ready {
        Ready(self.0.get() as usize)
    }
}

impl ops::BitOr for Interests {
    type Output = Self;

    #[inline]
    fn bitor(self, other: Self) -> Self {
        Interests(unsafe { NonZeroU8::new_unchecked(self.0.get() | other.0.get()) })
    }
}

impl ops::BitOrAssign for Interests {
    #[inline]
    fn bitor_assign(&mut self, other: Self) {
        self.0 = (*self | other).0;
    }
}

impl ops::Sub for Interests {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Interests(NonZeroU8::new(self.0.get() & !other.0.get()).unwrap())
    }
}

impl ops::SubAssign for Interests {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.0 = (*self - other).0;
    }
}

impl fmt::Debug for Interests {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut one = false;
        if self.is_readable() {
            if one {
                write!(fmt, " | ")?
            }
            write!(fmt, "{}", "READABLE")?;
            one = true
        }
        if self.is_writable() {
            if one {
                write!(fmt, " | ")?
            }
            write!(fmt, "{}", "WRITABLE")?;
            one = true
        }
        #[cfg(any(
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "ios",
            target_os = "macos"
        ))]
        {
            if self.is_aio() {
                if one {
                    write!(fmt, " | ")?
                }
                write!(fmt, "{}", "AIO")?;
                one = true
            }
        }
        #[cfg(any(target_os = "freebsd"))]
        {
            if self.is_lio() {
                if one {
                    write!(fmt, " | ")?
                }
                write!(fmt, "{}", "LIO")?;
                one = true
            }
        }
        debug_assert!(one, "printing empty interests");
        Ok(())
    }
}

#[test]
fn test_debug_interests() {
    assert_eq!(
        "READABLE | WRITABLE",
        format!("{:?}", Interests::readable() | Interests::writable())
    );
    assert_eq!("READABLE", format!("{:?}", Interests::readable()));
    assert_eq!("WRITABLE", format!("{:?}", Interests::writable()));
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    ))]
    {
        assert_eq!("AIO", format!("{:?}", Interests::aio()));
    }
    #[cfg(any(target_os = "freebsd"))]
    {
        assert_eq!("LIO", format!("{:?}", Interests::lio()));
    }
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
