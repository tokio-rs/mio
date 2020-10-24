/// Helper macro to execute a system call that returns an `io::Result`.
//
// Macro must be defined before any modules that uses them.
#[allow(unused_macros)]
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

cfg_os_poll! {
    mod net;

    mod selector;
    pub(crate) use self::selector::{event, Event, Events, Selector};

    mod sourcefd;
    pub use self::sourcefd::SourceFd;

    mod waker;
    pub(crate) use self::waker::Waker;

    cfg_tcp! {
        pub(crate) mod tcp;
    }

    cfg_udp! {
        pub(crate) mod udp;
    }

    cfg_uds! {
        pub(crate) mod uds;
        pub use self::uds::SocketAddr;
    }

    cfg_io_source! {
        use std::io;

        // Both `kqueue` and `epoll` don't need to hold any user space state.
        pub(crate) struct IoSourceState;

        impl IoSourceState {
            pub fn new() -> IoSourceState {
                IoSourceState
            }

            pub fn do_io<T, F, R>(&self, f: F, io: &T) -> io::Result<R>
            where
                F: FnOnce(&T) -> io::Result<R>,
            {
                // We don't hold state, so we can just call the function and
                // return.
                f(io)
            }
        }
    }

    cfg_pipe! {
        pub(crate) mod pipe;
    }
}

cfg_not_os_poll! {
    cfg_uds! {
        mod uds;
        pub use self::uds::SocketAddr;
    }

    cfg_any_os_util! {
        mod sourcefd;
        pub use self::sourcefd::SourceFd;
    }
}
