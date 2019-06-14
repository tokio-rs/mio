use crate::{sys, Registry, Token};
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
/// `Evented` values are always backed by **system** handles, which are backed
/// by sockets or other system handles. These `Evented` handles will be
/// monitored by the system selector. An implementation of `Evented` will almost
/// always delegates to a lower level handle.
///
/// [`Registry`]: ../struct.Registry.html
///
/// # Examples
///
/// Implementing `Evented` on a struct containing a socket:
///
/// ```
/// use mio::{Interests, Registry, Token};
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
///     fn register(&self, registry: &Registry, token: Token, interests: Interests)
///         -> io::Result<()>
///     {
///         // Delegate the `register` call to `socket`
///         self.socket.register(registry, token, interests)
///     }
///
///     fn reregister(&self, registry: &Registry, token: Token, interests: Interests)
///         -> io::Result<()>
///     {
///         // Delegate the `reregister` call to `socket`
///         self.socket.reregister(registry, token, interests)
///     }
///
///     fn deregister(&self, registry: &Registry) -> io::Result<()> {
///         // Delegate the `deregister` call to `socket`
///         self.socket.deregister(registry)
///     }
/// }
/// ```
pub trait Evented {
    /// Register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use [`Registry::register`]
    /// instead. Implementors should handle registration by delegating
    /// the call to another `Evented` type.
    ///
    /// [`Registry::register`]: ../struct.Registry.html#method.register
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()>;

    /// Re-register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use [`Registry::reregister`]
    /// instead. Implementors should handle re-registration by either delegating
    /// the call to another `Evented` type or calling
    /// [`SetReadiness::set_readiness`].
    ///
    /// [`Registry::reregister`]: ../struct.Registry.html#method.reregister
    /// [`SetReadiness::set_readiness`]: ../struct.SetReadiness.html#method.set_readiness
    fn reregister(&self, registry: &Registry, token: Token, interests: Interests)
        -> io::Result<()>;

    /// Deregister `self` from the given `Registry` instance
    ///
    /// This function should not be called directly. Use [`Registry::deregister`]
    /// instead. Implementors should handle deregistration by delegating
    /// the call to another `Evented` type.
    ///
    /// [`Registry::deregister`]: ../struct.Registry.html#method.deregister
    fn deregister(&self, registry: &Registry) -> io::Result<()>;
}

impl Evented for Box<dyn Evented> {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        self.as_ref().register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

impl<T: Evented> Evented for Box<T> {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        self.as_ref().register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

impl<T: Evented> Evented for ::std::sync::Arc<T> {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        self.as_ref().register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

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

// These must be unique.
const READABLE: u8 = 0b0_000_001;
const WRITABLE: u8 = 0b0_000_010;
// The following are not available on all platforms.
#[allow(dead_code)]
const ERROR: u8 = 0b0_000_100;
#[allow(dead_code)]
const HUP: u8 = 0b0_001_000;
#[allow(dead_code)]
const PRIORITY: u8 = 0b0_010_000;
#[allow(dead_code)]
const AIO: u8 = 0b0_100_000;
#[allow(dead_code)]
const LIO: u8 = 0b1_000_000;

impl Interests {
    /// Returns a `Interests` set representing readable interests.
    pub const READABLE: Interests = Interests(unsafe { NonZeroU8::new_unchecked(READABLE) });

    /// Returns a `Interests` set representing writable interests.
    pub const WRITABLE: Interests = Interests(unsafe { NonZeroU8::new_unchecked(WRITABLE) });

    /// Returns a `Interests` set representing AIO completion interests.
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    ))]
    pub const AIO: Interests = Interests(unsafe { NonZeroU8::new_unchecked(AIO) });

    /// Returns a `Interests` set representing LIO completion interests.
    #[cfg(target_os = "freebsd")]
    pub const LIO: Interests = Interests(unsafe { NonZeroU8::new_unchecked(LIO) });

    /// Returns true if the value includes readable readiness.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// let interests = Interests::READABLE;
    ///
    /// assert!(interests.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub fn is_readable(&self) -> bool {
        (self.0.get() & READABLE) != 0
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
    /// let interests = Interests::WRITABLE;
    ///
    /// assert!(interests.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub fn is_writable(&self) -> bool {
        (self.0.get() & WRITABLE) != 0
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
    /// let interests = Interests::AIO;
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
        (self.0.get() & AIO) != 0
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
    /// let interests = Interests::LIO;
    ///
    /// assert!(interests.is_lio());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    #[cfg(any(target_os = "freebsd"))]
    pub fn is_lio(&self) -> bool {
        (self.0.get() & LIO) != 0
    }

    #[cfg(windows)]
    pub(crate) fn to_ready(&self) -> Ready {
        Ready(self.0.get() as u8)
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
        format!("{:?}", Interests::READABLE | Interests::WRITABLE)
    );
    assert_eq!("READABLE", format!("{:?}", Interests::READABLE));
    assert_eq!("WRITABLE", format!("{:?}", Interests::WRITABLE));
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    ))]
    {
        assert_eq!("AIO", format!("{:?}", Interests::AIO));
    }
    #[cfg(any(target_os = "freebsd"))]
    {
        assert_eq!("LIO", format!("{:?}", Interests::LIO));
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
/// ```ignore
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
#[repr(transparent)]
pub struct Event {
    inner: sys::Event,
}

impl Event {
    /// Returns the event's token.
    pub fn token(&self) -> Token {
        self.inner.token()
    }

    /// Returns true if the `Ready` set contains readable readiness.
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.inner.is_readable()
    }

    /// Returns true if the `Ready` set contains writable readiness.
    #[inline]
    pub fn is_writable(&self) -> bool {
        self.inner.is_writable()
    }

    /// Returns true if the `Ready` set contains error readiness.
    ///
    /// Error events occur when the socket enters an error state. In this case,
    /// the socket will also receive a readable or writable event. Reading or
    /// writing to the socket will result in an error.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_error(&self) -> bool {
        self.inner.is_error()
    }

    /// Returns true if the `Ready` set contains HUP readiness.
    ///
    /// HUP events occur when the remote end of a socket hangs up. In the TCP
    /// case, this occurs when the remote end of a TCP socket shuts down writes.
    ///
    /// It is also unclear if HUP readiness will remain in 0.7. See
    /// [here](https://github.com/tokio-rs/mio/issues/941)
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_hup(&self) -> bool {
        self.inner.is_hup()
    }

    /// Returns true if the `Ready` set contains priority readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_priority(&self) -> bool {
        self.inner.is_priority()
    }

    /// Returns true if the `Ready` set contains AIO readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_aio(&self) -> bool {
        self.inner.is_aio()
    }

    /// Returns true if the `Ready` set contains LIO readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_lio(&self) -> bool {
        self.inner.is_lio()
    }

    /// Get access to the platform specific event, the returned value differs
    /// per platform.
    pub fn raw_event(&self) -> &sys::RawEvent {
        &self.inner.raw_event()
    }

    /// Create an `Event` from a platform specific event.
    pub fn from_raw_event(raw_event: sys::RawEvent) -> Event {
        Event {
            inner: sys::Event::from_raw_event(raw_event),
        }
    }
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
