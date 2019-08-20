use std::num::NonZeroU8;
use std::{fmt, ops};

/// Interests used in registering.
///
/// Interests are used in [registering] [`event::Source`]s with [`Poll`], they
/// indicate what readiness should be monitored for. For example if a socket is
/// registered with [readable] interests and the socket becomes writable, no
/// event will be returned from a call to [`poll`].
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
/// [`event::Source`]: crate::event::Source
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

    /// Add together two `Interests`.
    ///
    /// This does the same thing as the `BitOr` implementation, but is a
    /// constant function.
    ///
    /// ```
    /// use mio::Interests;
    ///
    /// const INTERESTS: Interests = Interests::READABLE.add(Interests::WRITABLE);
    /// # fn silent_dead_code_warning(_: Interests) { }
    /// # silent_dead_code_warning(INTERESTS)
    /// ```
    #[allow(clippy::should_implement_trait)]
    pub const fn add(self, other: Interests) -> Interests {
        Interests(unsafe { NonZeroU8::new_unchecked(self.0.get() | other.0.get()) })
    }

    /// Returns true if the value includes readable readiness.
    pub const fn is_readable(self) -> bool {
        (self.0.get() & READABLE) != 0
    }

    /// Returns true if the value includes writable readiness.
    pub const fn is_writable(self) -> bool {
        (self.0.get() & WRITABLE) != 0
    }

    /// Returns true if `Interests` contains AIO readiness
    pub const fn is_aio(self) -> bool {
        (self.0.get() & AIO) != 0
    }

    /// Returns true if `Interests` contains LIO readiness
    pub const fn is_lio(self) -> bool {
        (self.0.get() & LIO) != 0
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
            write!(fmt, "READABLE")?;
            one = true
        }
        if self.is_writable() {
            if one {
                write!(fmt, " | ")?
            }
            write!(fmt, "WRITABLE")?;
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
                write!(fmt, "AIO")?;
                one = true
            }
        }
        #[cfg(any(target_os = "freebsd"))]
        {
            if self.is_lio() {
                if one {
                    write!(fmt, " | ")?
                }
                write!(fmt, "LIO")?;
                one = true
            }
        }
        debug_assert!(one, "printing empty interests");
        Ok(())
    }
}
