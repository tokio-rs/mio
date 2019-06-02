use std::num::NonZeroU8;
use std::{fmt, ops};

use crate::Ready;

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
    #[doc(hidden)]
    pub fn to_ready(&self) -> Ready {
        Ready::from_usize(self.0.get() as usize)
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
