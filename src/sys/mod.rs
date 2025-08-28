//! Module with system specific types.
//!
//! Required types:
//!
//! * `Event`: a type alias for the system specific event, e.g. `kevent` or
//!   `epoll_event`.
//! * `event`: a module with various helper functions for `Event`, see
//!   [`crate::event::Event`] for the required functions.
//! * `Events`: collection of `Event`s, see [`crate::Events`].
//! * `IoSourceState`: state for the `IoSource` type.
//! * `Selector`: selector used to register event sources and poll for events,
//!   see [`crate::Poll`] and [`crate::Registry`] for required methods.
//! * `tcp` and `udp` modules: see the [`crate::net`] module.
//! * `Waker`: see [`crate::Waker`].

cfg_os_poll! {
    macro_rules! debug_detail {
        (
            $type: ident ($event_type: ty), $test: path,
            $($(#[$target: meta])* $libc: ident :: $flag: ident),+ $(,)*
        ) => {
            struct $type($event_type);

            impl fmt::Debug for $type {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    let mut written_one = false;
                    $(
                        $(#[$target])*
                        #[allow(clippy::bad_bit_mask)] // Apparently some flags are zero.
                        {
                            // Windows doesn't use `libc` but the `afd` module.
                            if $test(&self.0, &$libc :: $flag) {
                                if !written_one {
                                    write!(f, "{}", stringify!($flag))?;
                                    written_one = true;
                                } else {
                                    write!(f, "|{}", stringify!($flag))?;
                                }
                            }
                        }
                    )+
                    if !written_one {
                        write!(f, "(empty)")
                    } else {
                        Ok(())
                    }
                }
            }
        };
    }
}

#[cfg(any(unix, target_os = "hermit"))]
cfg_os_poll! {
    mod unix;
    #[allow(unused_imports)]
    pub use self::unix::*;
}

#[cfg(windows)]
cfg_os_poll! {
    mod windows;
    pub use self::windows::*;
}

#[cfg(target_os = "wasi")]
cfg_os_poll! {
    mod wasi;
    pub(crate) use self::wasi::*;
}

cfg_not_os_poll! {
    mod shell;
    pub(crate) use self::shell::*;

    #[cfg(unix)]
    cfg_any_os_ext! {
        mod unix;
        #[cfg(feature = "os-ext")]
        pub use self::unix::SourceFd;
    }
}

/// Define the `listen` backlog parameters as in the standard library. This
/// helps avoid hardcoded unsynchronized values and allows better control of
/// default values depending on the target.
///
/// Selecting a “valid” default value can be tricky due to:
///
/// - It often serves only as a hint and may be rounded, trimmed, or ignored by
///   the OS.
///
/// - It is sometimes provided as a "magic" value, for example, -1. This
///   value is undocumented and not standard, but it is often used to represent
///   the largest possible backlog size. This happens due to signed/unsigned
///   conversion and rounding to the upper bound performed by the OS.
///
/// - Default values vary depending on the OS and its version. Common defaults
///   include: -1, 128, 1024, and 4096.
///
// Here is the original code from the standard library
// https://github.com/rust-lang/rust/blob/4f808ba6bf9f1c8dde30d009e73386d984491587/library/std/src/os/unix/net/listener.rs#L72
//
#[allow(dead_code)]
#[cfg(any(
    target_os = "windows",
    target_os = "redox",
    target_os = "espidf",
    target_os = "horizon"
))]
pub(crate) const LISTEN_BACKLOG_SIZE: i32 = 128;

/// This is a special case for some target(s) supported by `mio`.  This value
/// is needed because `libc::SOMAXCON` (used as a fallback for unknown targets)
/// is not implemented for them. Feel free to update this if the `libc` crate
/// changes.
#[allow(dead_code)]
#[cfg(target_os = "hermit")]
pub(crate) const LISTEN_BACKLOG_SIZE: i32 = 1024;

#[allow(dead_code)]
#[cfg(any(
    // Silently capped to `/proc/sys/net/core/somaxconn`.
    target_os = "linux",
    // Silently capped to `kern.ipc.soacceptqueue`.
    target_os = "freebsd",
    // Silently capped to `kern.somaxconn sysctl`.
    target_os = "openbsd",
    // Silently capped to the default 128.
    target_vendor = "apple",
))]
pub(crate) const LISTEN_BACKLOG_SIZE: i32 = -1;

#[allow(dead_code)]
#[cfg(not(any(
    target_os = "windows",
    target_os = "redox",
    target_os = "espidf",
    target_os = "horizon",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "wasi",
    target_os = "hermit",
    target_vendor = "apple",
)))]
pub(crate) const LISTEN_BACKLOG_SIZE: i32 = libc::SOMAXCONN;
