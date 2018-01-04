use event_imp::{Ready, ready_as_usize, ready_from_usize};
use unix::UnixReady;
use super::super::ready::BSDReady;

use std::ops;
use std::fmt;

/// FreeBSD specific extensions to `Ready`
///
/// Provides additional readiness event kinds that are available on FreeBSD and
/// DragonFlyBSD.
///
/// LIO events occur when an lio_listio operation is complete.  Unlike other
/// event types, LIO events are not associated with file descriptors.  Instead,
/// they're associated with lists of AIO control block structures.
///
/// Conversion traits are implemented between `FreeBSDReady`, `BSDReady`,
/// `UnixReady`, and `Ready`.  See the examples.
///
/// # Examples
///
/// ```
/// use mio::unix::FreeBSDReady;
///
/// let ready = FreeBSDReady::lio();
///
/// assert!(FreeBSDReady::from(ready).is_lio());
/// ```
///
/// Basic conversion between ready types.
///
/// ```
/// use mio::Ready;
/// use mio::unix::FreeBSDReady;
///
/// // Start with a portable ready
/// let ready = Ready::empty();
/// // Convert to a BSD ready, add AIO
/// let mut bsd_ready = FreeBSDReady::from(ready) | FreeBSDReady::lio();
/// assert!(bsd_ready.is_lio());
/// // Convert back to `Ready`
/// let ready = Ready::from(bsd_ready);
/// ```
///
/// [readiness]: struct.Poll.html#readiness-operations
 #[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct FreeBSDReady(Ready);

const LIO: usize = 0b100000;

impl FreeBSDReady {
    /// Returns a `Ready` representing LIO completion readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::unix::FreeBSDReady;
    ///
    /// let ready = FreeBSDReady::lio();
    ///
    /// assert!(ready.is_lio());
    /// ```
    ///
    /// [`Poll`]: ../struct.Poll.html
    #[inline]
    pub fn lio() -> FreeBSDReady {
        FreeBSDReady(ready_from_usize(LIO))
    }

    /// Returns true if `Ready` contains LIO readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::unix::FreeBSDReady;
    ///
    /// let ready = FreeBSDReady::lio();
    ///
    /// assert!(ready.is_lio());
    /// ```
    ///
    /// [`Poll`]: ../struct.Poll.html
    #[inline]
    pub fn is_lio(&self) -> bool {
        self.contains(ready_from_usize(LIO))
    }
}

impl From<Ready> for FreeBSDReady {
    fn from(src: Ready) -> FreeBSDReady {
        FreeBSDReady(src)
    }
}

impl From<FreeBSDReady> for Ready {
    fn from(src: FreeBSDReady) -> Ready {
        src.0
    }
}

impl From<UnixReady> for FreeBSDReady {
    fn from(src: UnixReady) -> FreeBSDReady {
        FreeBSDReady(Ready::from(src))
    }
}

impl From<FreeBSDReady> for UnixReady {
    fn from(src: FreeBSDReady) -> UnixReady {
        UnixReady::from(src.0)
    }
}

impl From<BSDReady> for FreeBSDReady {
    fn from(src: BSDReady) -> FreeBSDReady {
        FreeBSDReady(Ready::from(src))
    }
}

impl From<FreeBSDReady> for BSDReady {
    fn from(src: FreeBSDReady) -> BSDReady {
        BSDReady::from(src.0)
    }
}

impl ops::Deref for FreeBSDReady {
    type Target = Ready;

    fn deref(&self) -> &Ready {
        &self.0
    }
}

impl ops::DerefMut for FreeBSDReady {
    fn deref_mut(&mut self) -> &mut Ready {
        &mut self.0
    }
}

impl ops::BitOr for FreeBSDReady {
    type Output = FreeBSDReady;

    #[inline]
    fn bitor(self, other: FreeBSDReady) -> FreeBSDReady {
        (self.0 | other.0).into()
    }
}

impl ops::BitXor for FreeBSDReady {
    type Output = FreeBSDReady;

    #[inline]
    fn bitxor(self, other: FreeBSDReady) -> FreeBSDReady {
        (self.0 ^ other.0).into()
    }
}

impl ops::BitAnd for FreeBSDReady {
    type Output = FreeBSDReady;

    #[inline]
    fn bitand(self, other: FreeBSDReady) -> FreeBSDReady {
        (self.0 & other.0).into()
    }
}

impl ops::Sub for FreeBSDReady {
    type Output = FreeBSDReady;

    #[inline]
    fn sub(self, other: FreeBSDReady) -> FreeBSDReady {
        ready_from_usize(ready_as_usize(self.0) & !ready_as_usize(other.0)).into()
    }
}

impl fmt::Debug for FreeBSDReady {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (Ready::readable(), "Readable"),
            (Ready::writable(), "Writable"),
            (Ready::from(UnixReady::error()), "Error"),
            (Ready::from(UnixReady::hup()), "Hup"),
            (Ready::from(BSDReady::aio()), "AIO"),
            (Ready::from(FreeBSDReady::lio()), "LIO"),
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
