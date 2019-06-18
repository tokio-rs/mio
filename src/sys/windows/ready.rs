use std::{fmt, ops};

use crate::Interests;

#[derive(Copy, Clone)]
pub struct Ready(u8);

// These must be the same as the values in for `Interests`, see
// `Ready::from_interests`.
const EMPTY: u8 = 0b0_000_000;
const READABLE: u8 = 0b0_000_001;
const WRITABLE: u8 = 0b0_000_010;
// The following are not available on all platforms.
const ERROR: u8 = 0b0_000_100;
const HUP: u8 = 0b0_001_000;
const PRIORITY: u8 = 0b0_010_000;
const AIO: u8 = 0b0_100_000;
const LIO: u8 = 0b1_000_000;

impl Ready {
    /// Returns an empty `Ready` set.
    pub const EMPTY: Ready = Ready(EMPTY);

    /// Returns a `Ready` set representing readable readiness.
    pub const READABLE: Ready = Ready(READABLE);

    /// Returns a `Ready` set representing writable readiness.
    pub const WRITABLE: Ready = Ready(WRITABLE);

    /// Returns a `Ready` set representing error readiness.
    #[cfg(unix)]
    pub const ERROR: Ready = Ready(ERROR);

    /// Returns a `Ready` set representing HUP readiness.
    #[cfg(unix)]
    pub const HUP: Ready = Ready(HUP);

    /// Returns a `Ready` set representing priority readiness.
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
    pub const PRIORITY: Ready = Ready(PRIORITY);

    /// Returns a `Ready` set representing AIO completion readiness.
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    ))]
    pub const AIO: Ready = Ready(AIO);

    /// Returns a `Ready` set representing LIO completion readiness.
    #[cfg(any(target_os = "freebsd"))]
    pub const LIO: Ready = Ready(LIO);

    /// Returns true if the `Ready` set is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0 == EMPTY
    }

    /// Returns true if the `Ready` set contains readable readiness.
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.contains(Ready::READABLE)
    }

    /// Returns true if the `Ready` set contains writable readiness.
    #[inline]
    pub fn is_writable(&self) -> bool {
        self.contains(Ready::WRITABLE)
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
        self.contains(Ready(ERROR))
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
        self.contains(Ready(HUP))
    }

    /// Returns true if the `Ready` set contains priority readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_priority(&self) -> bool {
        self.contains(Ready(PRIORITY))
    }

    /// Returns true if the `Ready` set contains AIO readiness.
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
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_lio(&self) -> bool {
        self.contains(Ready(LIO))
    }

    /// Returns true if `self` is a superset of `other`.
    ///
    /// The `other` set may represent more than one readiness operations, in
    /// which case the function only returns true if `self` contains **all**
    /// readiness specified in `other`.
    #[inline]
    pub fn contains(&self, other: Ready) -> bool {
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
    pub(crate) fn from_usize(val: usize) -> Ready {
        Ready(val as u8)
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
    pub(crate) fn as_usize(&self) -> usize {
        self.0 as usize
    }

    pub(crate) fn from_interests(interests: Interests) -> Ready {
        Ready(interests.as_u8())
    }
}

impl ops::BitOr for Ready {
    type Output = Ready;

    #[inline]
    fn bitor(self, other: Ready) -> Ready {
        Ready(self.0 | other.0)
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

impl fmt::Debug for Ready {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut one = false;
        let flags = [
            (Ready(READABLE), "Readable"),
            (Ready(WRITABLE), "Writable"),
            (Ready(ERROR), "Error"),
            (Ready(HUP), "Hup"),
            (Ready(PRIORITY), "Priority"),
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
