mod afd;
mod io_status_block;

pub mod event;
pub use event::{Event, Events};

mod ntapi;
pub(crate) use ntapi::{
    IO_STATUS_BLOCK_u, NtCancelIoFileEx, NtDeviceIoControlFile, RtlNtStatusToDosError,
    IO_STATUS_BLOCK,
};

mod selector;
pub use selector::{Selector, SelectorInner, SockState};

mod overlapped;
use overlapped::Overlapped;

// Macros must be defined before the modules that use them
cfg_net! {
    /// Helper macro to execute a system call that returns an `io::Result`.
    //
    // Macro must be defined before any modules that uses them.
    macro_rules! syscall {
        ($fn: ident ( $($arg: expr),* $(,)* ), $err_test: path, $err_value: expr) => {{
            let res = unsafe { $fn($($arg, )*) };
            if $err_test(&res, &$err_value) {
                Err(io::Error::last_os_error())
            } else {
                Ok(res)
            }
        }};
    }

    mod net;

    pub(crate) mod tcp;
    pub(crate) mod udp;

    pub(crate) use ntapi::{NtCreateFile, FILE_OPEN};
}

cfg_os_ext! {
    pub(crate) mod named_pipe;
}

mod waker;
pub(crate) use waker::Waker;

cfg_io_source! {
    use std::io;
    use std::os::windows::io::RawSocket;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use crate::{poll, Interest, Registry, Token};

    struct InternalState {
        selector: Arc<SelectorInner>,
        token: Token,
        interests: Interest,
        sock_state: Pin<Arc<Mutex<SockState>>>,
    }

    impl Drop for InternalState {
        fn drop(&mut self) {
            let mut sock_state = self.sock_state.lock().unwrap();
            sock_state.mark_delete();
        }
    }

    pub struct IoSourceState {
        // This is `None` if the socket has not yet been registered.
        //
        // We box the internal state to not increase the size on the stack as the
        // type might move around a lot.
        inner: Option<Box<InternalState>>,
    }

    impl IoSourceState {
        pub fn new() -> IoSourceState {
            IoSourceState { inner: None }
        }

        pub fn do_io<T, F, R>(&self, f: F, io: &T) -> io::Result<R>
        where
            F: FnOnce(&T) -> io::Result<R>,
        {
            let result = f(io);
            if let Err(ref e) = result {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.inner.as_ref().map_or(Ok(()), |state| {
                        state
                            .selector
                            .reregister(state.sock_state.clone(), state.token, state.interests)
                    })?;
                }
            }
            result
        }

        pub fn register(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
            socket: RawSocket,
        ) -> io::Result<()> {
            if self.inner.is_some() {
                Err(io::ErrorKind::AlreadyExists.into())
            } else {
                poll::selector(registry)
                    .register(socket, token, interests)
                    .map(|state| {
                        self.inner = Some(Box::new(state));
                    })
            }
        }

        pub fn reregister(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
        ) -> io::Result<()> {
            match self.inner.as_mut() {
                Some(state) => {
                    poll::selector(registry)
                        .reregister(state.sock_state.clone(), token, interests)
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
                    {
                        let mut sock_state = state.sock_state.lock().unwrap();
                        sock_state.mark_delete();
                    }
                    self.inner = None;
                    Ok(())
                }
                None => Err(io::ErrorKind::NotFound.into()),
            }
        }
    }
}
