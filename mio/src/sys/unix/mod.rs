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

    mod waker;
    pub(crate) use self::waker::Waker;

    mod sourcefd;
    pub use self::sourcefd::SourceFd;

    cfg_tcp! {
        mod tcp;
        pub(crate) use self::tcp::{TcpListener, TcpStream};
    }

    cfg_udp! {
        mod udp;
        pub(crate) use self::udp::UdpSocket;
    }

    cfg_uds! {
        mod uds;
        pub(crate) use self::uds::{UnixDatagram, UnixListener, UnixStream};
        pub use self::uds::SocketAddr;
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
