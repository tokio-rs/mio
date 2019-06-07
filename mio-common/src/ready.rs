use std::{fmt, ops};

/// A set of readiness event kinds
///
/// `Ready` is a set of operation descriptors indicating which kind of an
/// operation is ready to be performed. For example, `Ready::READABLE`
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
/// [readable]: Ready::READABLE
/// [writable]: Ready::WRITABLE
/// [error]: Ready::ERROR
/// [hup]: Ready::HUP
/// [priority]: Ready::PRIORITY
/// [AIO]: Ready::AIO
/// [LIO]: Ready::li0
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
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Ready(usize);

// These must be unique.
const EMPTY:    usize = 0b0_000_000;
const READABLE: usize = 0b0_000_001;
const WRITABLE: usize = 0b0_000_010;
// The following are not available on all platforms.
const ERROR:    usize = 0b0_000_100;
const HUP:      usize = 0b0_001_000;
const PRIORITY: usize = 0b0_010_000;
const AIO:      usize = 0b0_100_000;
const LIO:      usize = 0b1_000_000;

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
        self.0 == EMPTY
    }

    /// Returns true if the `Ready` set contains readable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::READABLE;
    /// assert!(ready.is_readable());
    /// ```
    #[inline]
    pub fn is_readable(&self) -> bool {
        self.contains(Ready::READABLE)
    }

    /// Returns true if the `Ready` set contains writable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::WRITABLE;
    /// assert!(ready.is_writable());
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::ERROR;
    /// assert!(ready.is_error());
    /// ```
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    #[inline]
    pub fn is_error(&self) -> bool {
        self.contains(Ready::ERROR)
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
    /// use mio::Ready;
    ///
    /// let ready = Ready::HUP;
    /// assert!(ready.is_hup());
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
    /// use mio::Ready;
    ///
    /// let ready = Ready::PRIORITY;
    /// assert!(ready.is_priority());
    /// ```
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
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::AIO;
    /// assert!(ready.is_aio());
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
    /// use mio::Ready;
    ///
    /// let ready = Ready::LIO;
    /// assert!(ready.is_lio());
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
    /// let mut readiness = Ready::EMPTY;
    /// readiness.insert(Ready::READABLE);
    /// assert!(readiness.is_readable());
    /// ```
    #[inline]
    pub fn insert(&mut self, other: Ready) {
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
    /// assert!(!readiness.is_readable());
    /// ```
    #[inline]
    pub fn remove(&mut self, other: Ready) {
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
    /// let readiness = Ready::READABLE;
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
    /// assert!(!Ready::READABLE.contains(readiness));
    /// assert!(readiness.contains(readiness));
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::Ready;
    ///
    /// let ready = Ready::READABLE;
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
    /// let ready = Ready::READABLE;
    /// let ready_usize = ready.as_usize();
    /// let ready2 = Ready::from_usize(ready_usize);
    /// assert_eq!(ready, ready2);
    /// ```
    pub fn as_usize(&self) -> usize {
        self.0
    }
}

impl ops::BitOr for Ready {
    type Output = Ready;

    #[inline]
    fn bitor(self, other: Ready) -> Ready {
        Ready(self.0 | other.0)
    }
}

impl ops::BitOrAssign for Ready {
    #[inline]
    fn bitor_assign(&mut self, other: Ready) {
        self.0 |= other.0;
    }
}

impl ops::BitXor for Ready {
    type Output = Ready;

    #[inline]
    fn bitxor(self, other: Ready) -> Ready {
        Ready(self.0 ^ other.0)
    }
}

impl ops::BitXorAssign for Ready {
    #[inline]
    fn bitxor_assign(&mut self, other: Ready) {
        self.0 ^= other.0;
    }
}

impl ops::BitAnd for Ready {
    type Output = Ready;

    #[inline]
    fn bitand(self, other: Ready) -> Ready {
        Ready(self.0 & other.0)
    }
}

impl ops::BitAndAssign for Ready {
    #[inline]
    fn bitand_assign(&mut self, other: Ready) {
        self.0 &= other.0
    }
}

impl ops::Sub for Ready {
    type Output = Ready;

    #[inline]
    fn sub(self, other: Ready) -> Ready {
        Ready(self.0 & !other.0)
    }
}

impl ops::SubAssign for Ready {
    #[inline]
    fn sub_assign(&mut self, other: Ready) {
        self.0 &= !other.0;
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

#[test]
fn fmt_debug() {
    assert_eq!("(empty)", format!("{:?}", Ready::EMPTY));
    assert_eq!("Readable", format!("{:?}", Ready::READABLE));
    assert_eq!("Writable", format!("{:?}", Ready::WRITABLE));
    assert_eq!("Error", format!("{:?}", Ready::ERROR));
    assert_eq!("Hup", format!("{:?}", Ready(HUP)));
    assert_eq!("Priority", format!("{:?}", Ready(PRI)));
    assert_eq!("AIO", format!("{:?}", Ready(AIO)));
    assert_eq!("LIO", format!("{:?}", Ready(LIO)));
    assert_eq!("Readable | Writable", format!("{:?}", Ready::READABLE | Ready::WRITABLE));
}

/* TODO(Thomas): check if this is still relevant.
#[test]
fn test_ready_all() {
    let readable = Ready::READABLE.as_usize();
    let writable = Ready::WRITABLE.as_usize();

    assert_eq!(
        READY_ALL | readable | writable,
        ERROR + HUP + AIO + LIO + PRI + readable + writable
    );

    // Issue #896.
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "solaris"))]
    assert!(!Ready::from(Ready::PRIORITY).is_writable());
}
*/
