/// Helper macro to execute a system call that returns an `io::Result`.
//
// Macro must be defined before any modules that uses them.
#[allow(unused_macros)]
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        #[allow(unused_unsafe)]
        let res = unsafe { libc::$fn($($arg, )*) };
        if res < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

cfg_os_poll! {
    #[cfg_attr(all(
        not(mio_unsupported_force_poll_poll),
        any(
            target_os = "android",
            target_os = "illumos",
            target_os = "linux",
            target_os = "redox",
        )
    ), path = "selector/epoll.rs")]
    #[cfg_attr(all(
        not(mio_unsupported_force_poll_poll),
        any(
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "ios",
            target_os = "macos",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "tvos",
            target_os = "visionos",
            target_os = "watchos",
        )
    ), path = "selector/kqueue.rs")]
    #[cfg_attr(any(
        mio_unsupported_force_poll_poll,
        target_os = "espidf",
        target_os = "fuchsia",
        target_os = "haiku",
        target_os = "hermit",
        target_os = "nto",
        target_os = "solaris",
        target_os = "vita",
    ), path = "selector/poll.rs")]
    mod selector;
    pub(crate) use self::selector::{event, Event, Events, Selector};

    cfg_io_source! {
        pub(crate) use self::selector::IoSourceState;
    }

    mod sourcefd;
    #[cfg(feature = "os-ext")]
    pub use self::sourcefd::SourceFd;

    mod waker;
    pub(crate) use self::waker::Waker;

    cfg_net! {
        mod net;

        pub(crate) mod tcp;
        pub(crate) mod udp;
        #[cfg(not(target_os = "hermit"))]
        pub(crate) mod uds;
    }

    #[cfg(any(
        // For the public `pipe` module, must match `cfg_os_ext` macro.
        all(feature = "os-ext", not(target_os = "hermit")),
        // For the `Waker` type based on a pipe.
        mio_unsupported_force_waker_pipe,
        target_os = "aix",
        target_os = "dragonfly",
        target_os = "haiku",
        target_os = "illumos",
        target_os = "netbsd",
        target_os = "nto",
        target_os = "openbsd",
        target_os = "redox",
        target_os = "solaris",
        target_os = "vita",
    ))]
    pub(crate) mod pipe;
}

cfg_not_os_poll! {
    cfg_any_os_ext! {
        mod sourcefd;
        #[cfg(feature = "os-ext")]
        pub use self::sourcefd::SourceFd;
    }
}
