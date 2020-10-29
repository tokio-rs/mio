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

/// One of the features enabled that needs `IoSource`. That is `net` or `pipe`
/// (on Unix).
macro_rules! cfg_io_source {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "net", all(unix, feature = "pipe")))]
            #[cfg_attr(docsrs, doc(any(feature = "net", all(unix, feature = "pipe"))))]
            $item
        )*
    }
}

/// Feature `pipe` enabled.
#[cfg(unix)]
macro_rules! cfg_pipe {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "pipe")]
            #[cfg_attr(docsrs, doc(cfg(feature = "pipe")))]
            $item
        )*
    }
}

/// Feature `os-util` enabled, or one of the features that need `os-util`.
macro_rules! cfg_any_os_util {
    ($($item:item)*) => {
        $(
            #[cfg(any(feature = "os-util", feature = "net", all(unix, feature = "pipe")))]
            #[cfg_attr(docsrs, doc(cfg(any(feature = "os-util", feature = "net", all(unix, feature = "pipe")))))]
            $item
        )*
    }
}
