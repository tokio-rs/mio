use std::{fmt, ops};

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
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Ready(usize);

const READABLE: usize = 0b00001;
const WRITABLE: usize = 0b00010;

// These are deprecated and are moved into platform specific implementations.
const ERROR: usize = 0b00100;
const HUP: usize = 0b01000;

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
    /// let ready = Ready::empty();
    ///
    /// assert!(!ready.is_readable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    pub fn empty() -> Ready {
        Ready(0)
    }

    /// Returns a `Ready` representing readable readiness.
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
    pub fn readable() -> Ready {
        Ready(READABLE)
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
    /// let ready = Ready::writable();
    ///
    /// assert!(ready.is_writable());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn writable() -> Ready {
        Ready(WRITABLE)
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
    /// let ready = Ready::empty();
    /// assert!(ready.is_empty());
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_empty(&self) -> bool {
        *self == Ready::empty()
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
        self.contains(Ready::readable())
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
        self.contains(Ready::writable())
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
    /// let mut readiness = Ready::empty();
    /// readiness.insert(Ready::readable());
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
    /// let mut readiness = Ready::readable();
    /// readiness.remove(Ready::readable());
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
    /// let readiness = Ready::readable();
    ///
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
    ///
    /// assert!(!Ready::readable().contains(readiness));
    /// assert!(readiness.contains(readiness));
    /// ```
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn contains<T: Into<Self>>(&self, other: T) -> bool {
        let other = other.into();
        (*self & other) == other
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
    ///
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
    ///
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
fn test_debug_ready() {
    assert_eq!("(empty)", format!("{:?}", Ready::empty()));
    assert_eq!("Readable", format!("{:?}", Ready::readable()));
    assert_eq!("Writable", format!("{:?}", Ready::writable()));
}
