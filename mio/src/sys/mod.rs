//! Module with system specific types.
//!
//! Required types:
//!
//! * `Event`: a type alias for the system specific event, e.g. `kevent` or
//!            `epoll_event`.
//! * `event`: a module with various helper functions for `Event`, see
//!            [`crate::event::Event`] for the required functions.
//! * `Events`: collection of `Event`s, see [`crate::Events`].
//! * `Selector`: selector used to register event sources and poll for events,
//!               see [`crate::Poll`] and [`crate::Registry`] for required
//!               methods.
//! * `TcpListener`, `TcpStream` and `UdpSocket`: see [`crate::net`] module.
//! * `Waker`: see [`crate::Waker`].

#[allow(unused_macros)]
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

#[cfg(unix)]
cfg_os_poll! {
    mod unix;

    pub(crate) use self::unix::{event, Event, Events, Selector};
    pub(crate) use self::unix::Waker;

    cfg_tcp! {
        pub(crate) use self::unix::{TcpListener, TcpStream};
    }

    cfg_udp! {
        pub(crate) use self::unix::UdpSocket;
    }

    cfg_uds! {
        pub(crate) use self::unix::{UnixDatagram, UnixListener, UnixStream};
        pub use self::unix::SocketAddr;
    }

    pub use self::unix::SourceFd;
}

#[cfg(windows)]
cfg_os_poll! {
    mod windows;
    pub(crate) use self::windows::*;
}

cfg_not_os_poll! {
    mod shell;
    pub(crate) use self::shell::*;

    #[cfg(unix)]
    cfg_any_os_util! {
        mod unix;
        pub use self::unix::SourceFd;
    }

    #[cfg(unix)]
    cfg_uds! {
        pub use self::unix::SocketAddr;
    }
}
