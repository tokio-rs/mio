use event_imp::{Ready, ready_as_usize, ready_from_usize};
use unix::UnixReady;

use std::ops;
use std::fmt;

/// BSD specific extensions to `Ready`
///
/// Provides additional readiness event kinds that are available on BSD
/// platforms.
///
/// AIO events occur when a POSIX AIO operation is complete.  Unlike other event
/// types, AIO events are not associated with file descriptors.  Instead,
/// they're associated with AIO control block structures.  Each operation has
/// its own control block structure, and each control block structure is
/// generally only used for a single operation.
///
/// Conversion traits are implemented between `BSDReady`, `UnixReady`, and
/// `Ready`.  See the examples.
///
/// # Examples
///
/// ```
/// use mio::unix::BSDReady;
///
/// let ready = BSDReady::aio();
///
/// assert!(BSDReady::from(ready).is_aio());
/// ```
///
/// Basic conversion between ready types.
///
/// ```
/// use mio::Ready;
/// use mio::unix::BSDReady;
///
/// // Start with a portable ready
/// let ready = Ready::empty();
/// // Convert to a BSD ready, add AIO
/// let mut bsd_ready = BSDReady::from(ready) | BSDReady::aio();
/// assert!(bsd_ready.is_aio());
/// // Convert back to `Ready`
/// let ready = Ready::from(bsd_ready);
/// ```
///
/// [readiness]: struct.Poll.html#readiness-operations
 #[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct BSDReady(Ready);

const AIO: usize = 0b010000;

impl BSDReady {
    /// Returns a `Ready` representing AIO completion readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::unix::BSDReady;
    ///
    /// let ready = BSDReady::aio();
    ///
    /// assert!(ready.is_aio());
    /// ```
    ///
    /// [`Poll`]: ../struct.Poll.html
    #[inline]
    pub fn aio() -> BSDReady {
        BSDReady(ready_from_usize(AIO))
    }

    /// Returns true if `Ready` contains AIO readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::unix::BSDReady;
    ///
    /// let ready = BSDReady::aio();
    ///
    /// assert!(ready.is_aio());
    /// ```
    ///
    /// [`Poll`]: ../struct.Poll.html
    #[inline]
    pub fn is_aio(&self) -> bool {
        self.contains(ready_from_usize(AIO))
    }
}

impl From<Ready> for BSDReady {
    fn from(src: Ready) -> BSDReady {
        BSDReady(src)
    }
}

impl From<BSDReady> for Ready {
    fn from(src: BSDReady) -> Ready {
        src.0
    }
}

impl From<UnixReady> for BSDReady {
    fn from(src: UnixReady) -> BSDReady {
        BSDReady(Ready::from(src))
    }
}

impl From<BSDReady> for UnixReady {
    fn from(src: BSDReady) -> UnixReady {
        UnixReady::from(src.0)
    }
}

impl ops::Deref for BSDReady {
    type Target = Ready;

    fn deref(&self) -> &Ready {
        &self.0
    }
}

impl ops::DerefMut for BSDReady {
    fn deref_mut(&mut self) -> &mut Ready {
        &mut self.0
    }
}

impl ops::BitOr for BSDReady {
    type Output = BSDReady;

    #[inline]
    fn bitor(self, other: BSDReady) -> BSDReady {
        (self.0 | other.0).into()
    }
}

impl ops::BitXor for BSDReady {
    type Output = BSDReady;

    #[inline]
    fn bitxor(self, other: BSDReady) -> BSDReady {
        (self.0 ^ other.0).into()
    }
}

impl ops::BitAnd for BSDReady {
    type Output = BSDReady;

    #[inline]
    fn bitand(self, other: BSDReady) -> BSDReady {
        (self.0 & other.0).into()
    }
}

impl ops::Sub for BSDReady {
    type Output = BSDReady;

    #[inline]
    fn sub(self, other: BSDReady) -> BSDReady {
        ready_from_usize(ready_as_usize(self.0) & !ready_as_usize(other.0)).into()
    }
}

impl fmt::Debug for BSDReady {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (Ready::readable(), "Readable"),
            (Ready::writable(), "Writable"),
            (Ready::from(UnixReady::error()), "Error"),
            (Ready::from(UnixReady::hup()), "Hup"),
            (Ready::from(BSDReady::aio()), "AIO"),
        ];

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
