use event_imp::{Ready, ready_as_usize, ready_from_usize};
pub use magenta_sys::{
    mx_signals_t,
    MX_OBJECT_READABLE,
    MX_OBJECT_WRITABLE,
};
use std::ops;

// The following impls are valid because Fuchsia and mio both represent
// "readable" as `1 << 0` and "writable" as `1 << 2`.
// We define this assertion here and call it from `Selector::new`,
// since `Selector:;new` is guaranteed to be called during a standard mio runtime,
// unlike the functions in this file.
#[inline]
pub fn assert_fuchsia_ready_repr() {
    debug_assert!(
        MX_OBJECT_READABLE.bits() as usize == ready_as_usize(Ready::READABLE),
        "Magenta MX_OBJECT_READABLE should have the same repr as Ready::READABLE"
    );
    debug_assert!(
        MX_OBJECT_WRITABLE.bits() as usize == ready_as_usize(Ready::WRITABLE),
        "Magenta MX_OBJECT_WRITABLE should have the same repr as Ready::WRITABLE"
    );
}

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
    /// Returns the `FuchsiaReady` as raw magenta signals.
    /// This function is just a more explicit, non-generic version of
    /// `FuchsiaReady::into`.
    #[inline]
    pub fn into_mx_signals(self) -> mx_signals_t {
        mx_signals_t::from_bits_truncate(ready_as_usize(self.0) as u32)
    }
}

impl Into<mx_signals_t> for FuchsiaReady {
    #[inline]
    fn into(self) -> mx_signals_t {
        self.into_mx_signals()
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

impl ops::BitOr<mx_signals_t> for FuchsiaReady {
    type Output = FuchsiaReady;

    #[inline]
    fn bitor(self, other: mx_signals_t) -> FuchsiaReady {
        self | FuchsiaReady::from(other)
    }
}

impl ops::BitXor<mx_signals_t> for FuchsiaReady {
    type Output = FuchsiaReady;

    #[inline]
    fn bitxor(self, other: mx_signals_t) -> FuchsiaReady {
        self ^ FuchsiaReady::from(other)
    }
}

impl ops::BitAnd<mx_signals_t> for FuchsiaReady {
    type Output = FuchsiaReady;

    #[inline]
    fn bitand(self, other: mx_signals_t) -> FuchsiaReady {
        self & FuchsiaReady::from(other)
    }
}

impl ops::Sub<mx_signals_t> for FuchsiaReady {
    type Output = FuchsiaReady;

    #[inline]
    fn sub(self, other: mx_signals_t) -> FuchsiaReady {
        self - FuchsiaReady::from(other)
    }
}
