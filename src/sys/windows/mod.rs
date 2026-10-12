mod afd;

pub mod event;
pub use event::{Event, Events};

mod handle;
use handle::Handle;

mod io_status_block;
mod iocp;

mod overlapped;
use overlapped::Overlapped;

mod selector;
pub use selector::Selector;

// Macros must be defined before the modules that use them
cfg_net! {
    /// Helper macro to execute a system call that returns an `io::Result`.
    //
    // Macro must be defined before any modules that use them.
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

    pub use selector::{SelectorInner, SockState};
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

    use crate::{Interest, Registry, Token};

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

        /// Same as [`do_io`], for an operation known to only involve
        /// `interest`.
        ///
        /// Re-arming the full registered interest after a blocked read also
        /// re-requests write readiness. The AFD poll completes immediately for
        /// a writable socket, so that produces a writable event the caller
        /// never asked for, on every blocked read. See #1963.
        ///
        /// [`do_io`]: IoSourceState::do_io
        #[cfg(feature = "net")]
        pub fn do_io_with<T, F, R>(&self, interest: Interest, f: F, io: &T) -> io::Result<R>
        where
            F: FnOnce(&T) -> io::Result<R>,
        {
            let result = f(io);
            if let Err(ref e) = result {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.inner.as_ref().map_or(Ok(()), |state| {
                        // Never re-arm more than the source is registered for.
                        // If the blocked direction is not registered at all
                        // there is nothing to narrow to, so fall back to the
                        // full interest, which is what `do_io` would do.
                        let interests = match (
                            interest.is_readable() && state.interests.is_readable(),
                            interest.is_writable() && state.interests.is_writable(),
                        ) {
                            (true, false) => Interest::READABLE,
                            (false, true) => Interest::WRITABLE,
                            _ => state.interests,
                        };

                        state
                            .selector
                            .rearm(state.sock_state.clone(), state.token, interests)
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
                registry
                    .selector()
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
                    registry
                        .selector()
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
