//! Macros to ease conditional code based on enabled features.

// Depending on the features not all macros are used.
#![allow(unused_macros)]

/// Feature `os-poll` enabled.
macro_rules! cfg_os_poll {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "os-poll")]
            #[cfg_attr(docsrs, doc(cfg(feature = "os-poll")))]
            $item
        )*
    }
}

/// Feature `os-poll` disabled.
macro_rules! cfg_not_os_poll {
    ($($item:item)*) => {
        $(
            #[cfg(not(feature = "os-poll"))]
            $item
        )*
    }
}

/// One of the `tcp`, `udp`, `uds` features enabled.
#[cfg(unix)]
macro_rules! cfg_net {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "tcp", feature = "udp", feature = "uds"))]
            #[cfg_attr(docsrs, doc(cfg(any(feature = "tcp", feature = "udp", feature = "uds"))))]
            $item
        )*
    }
}

/// One of the `tcp`, `udp` features enabled.
#[cfg(windows)]
macro_rules! cfg_net {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "tcp", feature = "udp"))]
            #[cfg_attr(docsrs, doc(cfg(any(feature = "tcp", feature = "udp"))))]
            $item
        )*
    }
}

/// Feature `tcp` enabled.
macro_rules! cfg_tcp {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "tcp")]
            #[cfg_attr(docsrs, doc(cfg(feature = "tcp")))]
            $item
        )*
    }
}

/// Feature `udp` enabled.
macro_rules! cfg_udp {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "udp")]
            #[cfg_attr(docsrs, doc(cfg(feature = "udp")))]
            $item
        )*
    }
}

/// Feature `uds` enabled.
#[cfg(unix)]
macro_rules! cfg_uds {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "uds")]
            #[cfg_attr(docsrs, doc(cfg(feature = "uds")))]
            $item
        )*
    }
}

/// Feature `os-util` enabled, or one of the features that need `os-util`.
#[cfg(unix)]
macro_rules! cfg_any_os_util {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "os-util", feature = "tcp", feature = "udp", feature = "uds"))]
            #[cfg_attr(docsrs, doc(cfg(any(feature = "os-util", feature = "tcp", feature = "udp", feature = "uds"))))]
            $item
        )*
    }
}

/// Feature `os-util` enabled, or one of the features that need `os-util`.
#[cfg(windows)]
macro_rules! cfg_any_os_util {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "os-util", feature = "tcp", feature = "udp"))]
            #[cfg_attr(docsrs, doc(cfg(any(feature = "os-util", feature = "tcp", feature = "udp"))))]
            $item
        )*
    }
}

/// OS supports epoll(7) interface
macro_rules! cfg_epoll {
    ($($item:item)*) => {
        $(
            #[cfg(all(
                any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "illumos",
                ),
                feature = "os-epoll",
            ))]
            $item
        )*
    }
}

/// OS supports kqueue(2) interface
macro_rules! cfg_kqueue {
    ($($item:item)*) => {
        $(
            #[cfg(all(
                any(
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "ios",
                    target_os = "macos",
                    target_os = "netbsd",
                    target_os = "openbsd",
                ),
                feature = "os-kqueue"
            ))]
            $item
        )*
    }
}

/// OS supports either epoll(7) or kqueue(2) interface
macro_rules! cfg_epoll_or_kqueue {
    ($($item:item)*) => {
        $(
            #[cfg(any(
                all(
                    any(
                        target_os = "linux",
                        target_os = "android",
                        target_os = "illumos",
                    ),
                    feature = "os-epoll",
                ),
                all(
                    any(
                        target_os = "dragonfly",
                        target_os = "freebsd",
                        target_os = "ios",
                        target_os = "macos",
                        target_os = "netbsd",
                        target_os = "openbsd",
                    ),
                    feature = "os-kqueue"
                )
            ))]
            $item
        )*
    }
}

/// OS neither supports epoll(7) nor kqueue(2) interfaces
macro_rules! cfg_neither_epoll_nor_kqueue {
    ($($item:item)*) => {
        $(
            #[cfg(not(any(
                all(
                    any(
                        target_os = "linux",
                        target_os = "android",
                        target_os = "illumos",
                    ),
                    feature = "os-epoll",
                ),
                all(
                    any(
                        target_os = "dragonfly",
                        target_os = "freebsd",
                        target_os = "ios",
                        target_os = "macos",
                        target_os = "netbsd",
                        target_os = "openbsd",
                    ),
                    feature = "os-kqueue"
                )
            )))]
            $item
        )*
    }
}

/// Enable Waker backed by kqueue(2) interface
macro_rules! cfg_kqueue_waker {
    ($($item:item)*) => {
        $(
            #[cfg(all(
                any(
                    target_os = "freebsd",
                    target_os = "ios",
                    target_os = "macos",
                ),
                feature = "os-kqueue"
            ))]
            $item
        )*
    }
}

/// Enable Waker backed by pipe(2) interface
macro_rules! cfg_pipe_waker {
    ($($item:item)*) => {
        $(
            #[cfg(not(any(
                all(
                    any(
                        target_os = "linux",
                        target_os = "android",
                        target_os = "illumos",
                    ),
                    feature = "os-epoll",
                ),
                all(
                    any(
                        target_os = "freebsd",
                        target_os = "ios",
                        target_os = "macos",
                    ),
                    feature = "os-kqueue"
                )
            )))]
            $item
        )*
    }
}
