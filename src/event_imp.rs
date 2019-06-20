use crate::sys::SysEvent;
use crate::{sys, Registry, Token};
use std::num::NonZeroU8;
use std::{fmt, io, ops};

/// A value that may be registered with [`Registry`].
///
/// Handles that implement `Evented` can be registered with `Registry`. Users of
/// Mio **should not** use the `Evented` trait functions directly. Instead, the
/// equivalent functions on `Registry` should be used.
///
/// See [`Registry`] for more details.
///
/// [`Registry`]: crate::Registry
///
/// # Implementing `Evented`
///
/// `Evented` values are always backed by **system** handles, which are backed
/// by sockets or other system handles. These `Evented` handles will be
/// monitored by the system selector. An implementation of `Evented` will almost
/// always delegates to a lower level handle. Examples of this are
/// [`TcpStream`]s, or the *unix only* [`EventedFd`].
///
/// [`TcpStream`]: crate::net::TcpStream
/// [`EventedFd`]: crate::unix::EventedFd
///
/// # Dropping `Evented` types
///
/// All `Evented` types, unless otherwise specified, need to be [deregistered]
/// before being dropped for them to not leak resources. This goes against the
/// normal drop behaviour of types in Rust which cleanup after themselves, e.g.
/// a `File` will close itself. However since deregistering needs access to
/// [`Registry`] this cannot be done while being dropped.
///
/// [deregistered]: crate::Registry::deregister
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
/// Interests are used in [registering] [`Evented`] handles with [`Poll`],
/// they indicate what readiness should be monitored for. For example if a
/// socket is registered with [readable] interests and the socket becomes
/// writable, no event will be returned from a call to [`poll`].
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
/// [registering]: crate::Registry::register
/// [`Poll`]: crate::Poll
/// [readable]: Interests::READABLE
/// [`poll`]: crate::Poll::poll
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Interests(NonZeroU8);

// These must be unique.
const READABLE: u8 = 0b0_001;
const WRITABLE: u8 = 0b0_010;
// The following are not available on all platforms.
#[cfg_attr(
    not(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    )),
    allow(dead_code)
)]
const AIO: u8 = 0b0_100;
#[cfg_attr(not(target_os = "freebsd"), allow(dead_code))]
const LIO: u8 = 0b1_000;

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
    pub fn is_readable(&self) -> bool {
        (self.0.get() & READABLE) != 0
    }

    /// Returns true if the value includes writable readiness.
    pub fn is_writable(&self) -> bool {
        (self.0.get() & WRITABLE) != 0
    }

    /// Returns true if `Interests` contains AIO readiness
    pub fn is_aio(&self) -> bool {
        (self.0.get() & AIO) != 0
    }

    /// Returns true if `Interests` contains LIO readiness
    pub fn is_lio(&self) -> bool {
        (self.0.get() & LIO) != 0
    }

    #[cfg(windows)]
    pub(crate) fn as_u8(self) -> u8 {
        self.0.get()
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

/// A readiness event.
///
/// `Event` is a readiness state paired with a [`Token`]. It is returned by
/// [`Poll::poll`].
///
/// For more documentation on polling and events, see [`Poll`].
///
/// [`Poll::poll`]: crate::Poll::poll
/// [`Poll`]: crate::Poll
/// [`Token`]: crate::Token
#[repr(transparent)]
pub struct Event {
    inner: sys::Event,
}

impl Event {
    /// Returns the event's token.
    #[inline]
    pub fn token(&self) -> Token {
        self.inner.token()
    }

    /// Returns true if the event contains readable readiness.
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.inner.is_readable()
    }

    /// Returns true if the event contains writable readiness.
    #[inline]
    pub fn is_writable(&self) -> bool {
        self.inner.is_writable()
    }

    /// Returns true if the event contains error readiness.
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

    /// Returns true if the event contains HUP readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    ///
    /// When HUP events are received is different per platform. For example on
    /// epoll platforms HUP will be received when a TCP socket reached EOF,
    /// however on kqueue platforms a HUP event will be received when `shutdown`
    /// is called on the connection (both when shutting down the reading
    /// **and/or** writing side). Meaning that even though this function might
    /// return true on one platform in a given situation, it might return false
    /// on a different platform in the same situation. Furthermore even if true
    /// was returned on both platforms it could simply mean something different
    /// on the two platforms.
    ///
    /// Because of the above be cautions when using this in cross-platform
    /// applications, Mio makes no attempt at normalising this indicator and
    /// only provides a convenience method to read it.
    #[inline]
    pub fn is_hup(&self) -> bool {
        self.inner.is_hup()
    }

    /// Returns true if the event contains priority readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_priority(&self) -> bool {
        self.inner.is_priority()
    }

    /// Returns true if the event contains AIO readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_aio(&self) -> bool {
        self.inner.is_aio()
    }

    /// Returns true if the event contains LIO readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_lio(&self) -> bool {
        self.inner.is_lio()
    }

    /// Create an `Event` from a platform specific event.
    pub(crate) fn from_sys_event(sys_event: SysEvent) -> Event {
        Event {
            inner: sys::Event::from_sys_event(sys_event),
        }
    }
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Event")
            .field("token", &self.token())
            .field("readable", &self.is_readable())
            .field("writable", &self.is_writable())
            .field("error", &self.is_error())
            .field("hup", &self.is_hup())
            .field("priority", &self.is_priority())
            .field("aio", &self.is_aio())
            .field("lio", &self.is_lio())
            .finish()
    }
}

#[cfg(test)]
mod interests_tests {
    use super::Interests;

    #[test]
    fn is_tests() {
        assert!(Interests::READABLE.is_readable());
        assert!(!Interests::READABLE.is_writable());
        assert!(!Interests::WRITABLE.is_readable());
        assert!(Interests::WRITABLE.is_writable());
        assert!(!Interests::WRITABLE.is_aio());
        assert!(!Interests::WRITABLE.is_lio());
    }

    #[test]
    fn bit_or() {
        let interests = Interests::READABLE | Interests::WRITABLE;
        assert!(interests.is_readable());
        assert!(interests.is_writable());
    }

    #[test]
    fn fmt_debug() {
        assert_eq!(format!("{:?}", Interests::READABLE), "READABLE");
        assert_eq!(format!("{:?}", Interests::WRITABLE), "WRITABLE");
        assert_eq!(
            format!("{:?}", Interests::READABLE | Interests::WRITABLE),
            "READABLE | WRITABLE"
        );
        #[cfg(any(
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "ios",
            target_os = "macos"
        ))]
        {
            assert_eq!(format!("{:?}", Interests::AIO), "AIO");
        }
        #[cfg(any(target_os = "freebsd"))]
        {
            assert_eq!(format!("{:?}", Interests::LIO), "LIO");
        }
    }
}
