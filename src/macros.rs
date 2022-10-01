//! Macros to ease conditional code based on enabled features.

// Depending on the features not all macros are used.
#![allow(unused_macros)]

/// The `os-poll` feature is enabled.
macro_rules! cfg_os_poll {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "os-poll")]
            #[cfg_attr(docsrs, doc(cfg(feature = "os-poll")))]
            $item
        )*
    }
}

/// The `os-poll` feature is disabled.
macro_rules! cfg_not_os_poll {
    ($($item:item)*) => {
        $(
            #[cfg(not(feature = "os-poll"))]
            $item
        )*
    }
}

/// The `os-ext` feature is enabled.
macro_rules! cfg_os_ext {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "os-ext")]
            #[cfg_attr(docsrs, doc(cfg(feature = "os-ext")))]
            $item
        )*
    }
}

/// The `net` feature is enabled.
macro_rules! cfg_net {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "net")]
            #[cfg_attr(docsrs, doc(cfg(feature = "net")))]
            $item
        )*
    }
}

/// One of the features enabled that needs `IoSource`. That is `net` or `os-ext`
/// on Unix (for `pipe`).
macro_rules! cfg_io_source {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "net", all(unix, feature = "os-ext")))]
            #[cfg_attr(docsrs, doc(cfg(any(feature = "net", all(unix, feature = "os-ext")))))]
            $item
        )*
    }
}

/// The `os-ext` feature is enabled, or one of the features that need `os-ext`.
macro_rules! cfg_any_os_ext {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "os-ext", feature = "net"))]
            #[cfg_attr(docsrs, doc(cfg(any(feature = "os-ext", feature = "net"))))]
            $item
        )*
    }
}

/// The `os-poll` feature or one feature that requires is is enabled and the system
/// supports epoll.
macro_rules! cfg_epoll_selector {
    ($($item:item)*) => {
        $(
            #[cfg(all(
                any(feature = "os-poll", feature = "net"),
                any(
                    target_os = "android",
                    target_os = "illumos",
                    target_os = "linux",
                    target_os = "redox",
                ),
                not(feature = "force-old-poll")
            ))]
            $item
        )*
    };
}

/// The `os-poll` feature or one feature that requires is is enabled and the system
/// supports kqueue.
macro_rules! cfg_kqueue_selector {
    ($($item:item)*) => {
        $(
            #[cfg(all(
                any(feature = "os-poll", feature = "net"),
                any(
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "ios",
                    target_os = "macos",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ),
                not(feature = "force-old-poll")
            ))]
            $item
        )*
    };
}

/// The `os-poll` feature or one feature that requires is is enabled and the system
/// is a generic unix which does not support epoll nor kqueue.
macro_rules! cfg_poll_selector {
    ($($item:item)*) => {
        $(
            #[cfg(
                all(
                    unix,
                    any(feature = "os-poll", feature = "net"),
                    any(
                        not(any(
                            target_os = "android",
                            target_os = "illumos",
                            target_os = "linux",
                            target_os = "redox",
                            target_os = "dragonfly",
                            target_os = "freebsd",
                            target_os = "ios",
                            target_os = "macos",
                            target_os = "netbsd",
                            target_os = "openbsd"
                        )),
                        feature = "force-old-poll"
                    )
                )
            )]
            $item
        )*
    };
}
