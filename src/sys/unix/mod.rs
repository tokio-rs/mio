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

    cfg_epoll_or_kqueue! {
        cfg_net! {
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
    }

    cfg_neither_epoll_nor_kqueue! {
        cfg_net! {
            use std::io;
            use crate::{poll, Interest, Registry, Token};
            use std::os::unix::io::RawFd;

            #[derive(Debug)]
            struct InternalState {
                selector: Selector,
                token: Token,
                interests: Interest,
                socket: RawFd,
            }

            pub struct IoSourceState {
                inner: Option<Box<InternalState>>,
            }

            impl IoSourceState  {
                pub fn new() -> IoSourceState {
                    IoSourceState { inner: None }
                }

                pub fn do_io<T, F, R>(&self, f: F, io: &T) -> io::Result<R>
                where
                    F: FnOnce(&T) -> io::Result<R>,
                {
                    let result = f(io);
                    self.inner.as_ref().map_or(Ok(()), |state|  {
                        state
                            .selector
                            .rearm(state.socket, state.interests)
                    })?;
                    result
                }

                pub fn register(&mut self,
                    registry: &Registry,
                    token: Token,
                    interests: Interest,
                    socket: RawFd
                ) -> io::Result<()> {
                    if self.inner.is_some() {
                        Err(io::ErrorKind::AlreadyExists.into())
                    } else {
                        let selector = poll::selector(registry);
                        selector
                            .register(socket, token, interests)
                            .map(|_state| {
                                self.inner = Some(Box::new(InternalState {
                                    selector: selector.try_clone().unwrap(),
                                    socket, token, interests
                                }));
                            })
                    }
                }

                pub fn reregister(
                    &mut self,
                    _registry: &Registry,
                    token: Token,
                    interests: Interest,
                ) -> io::Result<()> {
                    match self.inner.as_mut() {
                        Some(state) => {
                            state.selector
                                .reregister(state.socket, token, interests)
                                .map(|()| {
                                    state.token = token;
                                    state.interests = interests;
                                })
                        }
                        None => Err(io::ErrorKind::NotFound.into()),
                    }
                }

                pub fn deregister(&mut self) -> io::Result<()> {
                    match self.inner.as_mut() {
                        Some(state) => {
                            state.selector.deregister(state.socket).unwrap();
                            self.inner = None;
                            Ok(())
                        }
                        None => Err(io::ErrorKind::NotFound.into()),
                    }
                }
            }
        }
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
