macro_rules! os_required {
    () => {
        panic!("mio must be compiled with `os-poll` to run.")
    };
}

mod selector;
pub(crate) use self::selector::{event, Event, Events, Selector};

mod waker;
pub(crate) use self::waker::Waker;

cfg_tcp! {
    pub(crate) mod tcp;
}

cfg_udp! {
    pub(crate) mod udp;
}

#[cfg(unix)]
cfg_uds! {
    pub(crate) mod uds;
}

cfg_net! {
    use std::io;

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

cfg_neither_epoll_nor_kqueue! {
    use crate::{Registry, Token, Interest};

    #[cfg(windows)]
    use std::os::windows::io::RawSocket;

    #[cfg(not(windows))]
    use std::os::unix::io::RawFd;

    impl IoSourceState {
        #[cfg(windows)]
        pub fn register(
            &mut self,
            _: &Registry,
            _: Token,
            _: Interest,
            _: RawSocket,
        ) -> io::Result<()> {
            os_required!()
        }

        #[cfg(not(windows))]
        pub fn register(
            &mut self,
            _: &Registry,
            _: Token,
            _: Interest,
            _: RawFd,
        ) -> io::Result<()> {
            os_required!()
        }

        pub fn reregister(
            &mut self,
            _: &Registry,
            _: Token,
            _: Interest,
        ) -> io::Result<()> {
           os_required!()
        }

        pub fn deregister(&mut self) -> io::Result<()> {
            os_required!()
        }
    }
} // cfg_neither_epoll_nor_kqueue!
}
