use std::path::Path;
use std::{ascii, fmt};
use crate::sys;

/// An address associated with a `mio` specific Unix socket.
///
/// This is implemented instead of imported from [`net::SocketAddr`] because
/// there is no way to create a [`net::SocketAddr`]. One must be returned by
/// [`accept`], so this is returned instead.
///
/// [`net::SocketAddr`]: std::os::unix::net::SocketAddr
/// [`accept`]: #method.accept
pub struct SocketAddr {
    inner: sys::SocketAddr
}

struct AsciiEscaped<'a>(&'a [u8]);

pub(crate) enum AddressKind<'a> {
    Unnamed,
    Pathname(&'a Path),
    Abstract(&'a [u8]),
}

impl SocketAddr {
    pub(crate) fn new(inner: sys::SocketAddr) -> Self {
        SocketAddr { inner }
    }

    fn address(&self) -> AddressKind<'_> {
        self.inner.address()
    }
}

cfg_os_poll! {
    impl SocketAddr {
        /// Returns `true` if the address is unnamed.
        ///
        /// Documentation reflected in [`SocketAddr`]
        ///
        /// [`SocketAddr`]: std::os::unix::net::SocketAddr
        pub fn is_unnamed(&self) -> bool {
            matches!(self.address(), AddressKind::Unnamed)
        }

        /// Returns the contents of this address if it is a `pathname` address.
        ///
        /// Documentation reflected in [`SocketAddr`]
        ///
        /// [`SocketAddr`]: std::os::unix::net::SocketAddr
        pub fn as_pathname(&self) -> Option<&Path> {
            if let AddressKind::Pathname(path) = self.address() {
                Some(path)
            } else {
                None
            }
        }

        /// Returns the contents of this address if it is an abstract namespace
        /// without the leading null byte.
        // Link to std::os::unix::net::SocketAddr pending
        // https://github.com/rust-lang/rust/issues/85410.
        pub fn as_abstract_namespace(&self) -> Option<&[u8]> {
            if let AddressKind::Abstract(path) = self.address() {
                Some(path)
            } else {
                None
            }
        }
    }
}

impl fmt::Debug for SocketAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{:?}", self.address())
    }
}

impl fmt::Debug for AddressKind<'_> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressKind::Unnamed => write!(fmt, "(unnamed)"),
            AddressKind::Abstract(name) => write!(fmt, "{} (abstract)", AsciiEscaped(name)),
            AddressKind::Pathname(path) => write!(fmt, "{:?} (pathname)", path),
        }
    }
}

impl<'a> fmt::Display for AsciiEscaped<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "\"")?;
        for byte in self.0.iter().cloned().flat_map(ascii::escape_default) {
            write!(fmt, "{}", byte as char)?;
        }
        write!(fmt, "\"")
    }
}
