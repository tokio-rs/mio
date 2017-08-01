use event_imp::{Ready, ready_as_usize, ready_from_usize};
pub use magenta_sys::mx_signals_t;
use std::ops;

// The following impls are valid because Fuchsia and mio both represent
// "readable" as `1 << 0` and "writable" as `1 << 2`.

/// Fuchsia specific extensions to `Ready`
///
/// Provides additional readiness event kinds that are available on Fuchsia.
///
/// Conversion traits are implemented between `Ready` and `FuchsiaReady`.
///
/// For high level documentation on polling and readiness, see [`Poll`].
///
/// [`Poll`]: struct.Poll.html
#[derive(Debug, Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct FuchsiaReady(Ready);

impl FuchsiaReady {
    /// Returns whether or not the `FuchsiaReady` contains all of the specified
    /// magenta signals.
    #[inline]
    pub fn has_all(&self, signals: mx_signals_t) -> bool {
        (Ready::from(signals) - self.0).is_empty()
    }

    /// Returns whether or not the `FuchsiaReady` contains any of the specified
    /// magenta signals.
    #[inline]
    pub fn has_any(&self, signals: mx_signals_t) -> bool {
        !(Ready::from(signals) & self.0).is_empty()
    }

    /// Returns the `FuchsiaReady` as raw magenta signals.
    /// This function is just a more explicit, non-generic version of
    /// `FuchsiaReady::into`.
    #[inline]
    pub fn into_raw_signals(self) -> mx_signals_t {
        mx_signals_t::from_bits_truncate(ready_as_usize(self.0) as u32)
    }
}

impl Into<mx_signals_t> for FuchsiaReady {
    #[inline]
    fn into(self) -> mx_signals_t {
        self.into_raw_signals()
    }
}

impl From<mx_signals_t> for FuchsiaReady {
    #[inline]
    fn from(src: mx_signals_t) -> Self {
        FuchsiaReady(src.into())
    }
}

impl From<mx_signals_t> for Ready {
    #[inline]
    fn from(src: mx_signals_t) -> Self {
        ready_from_usize(src.bits() as usize)
    }
}

impl From<Ready> for FuchsiaReady {
    #[inline]
    fn from(src: Ready) -> FuchsiaReady {
        FuchsiaReady(src)
    }
}

impl From<FuchsiaReady> for Ready {
    #[inline]
    fn from(src: FuchsiaReady) -> Ready {
        src.0
    }
}

impl ops::Deref for FuchsiaReady {
    type Target = Ready;

    #[inline]
    fn deref(&self) -> &Ready {
        &self.0
    }
}

impl ops::DerefMut for FuchsiaReady {
    #[inline]
    fn deref_mut(&mut self) -> &mut Ready {
        &mut self.0
    }
}

impl ops::BitOr for FuchsiaReady {
    type Output = FuchsiaReady;

    #[inline]
    fn bitor(self, other: FuchsiaReady) -> FuchsiaReady {
        (self.0 | other.0).into()
    }
}

impl ops::BitXor for FuchsiaReady {
    type Output = FuchsiaReady;

    #[inline]
    fn bitxor(self, other: FuchsiaReady) -> FuchsiaReady {
        (self.0 ^ other.0).into()
    }
}

impl ops::BitAnd for FuchsiaReady {
    type Output = FuchsiaReady;

    #[inline]
    fn bitand(self, other: FuchsiaReady) -> FuchsiaReady {
        (self.0 & other.0).into()
    }
}

impl ops::Sub for FuchsiaReady {
    type Output = FuchsiaReady;

    #[inline]
    fn sub(self, other: FuchsiaReady) -> FuchsiaReady {
        (self.0 & !other.0).into()
    }
}

impl ops::Not for FuchsiaReady {
    type Output = FuchsiaReady;

    #[inline]
    fn not(self) -> FuchsiaReady {
        (!self.0).into()
    }
}