//! Both `poll(2)` and `event_port(2)` need to hold per-fd states.

use std::sync::atomic::{AtomicBool, Ordering};

cfg_io_source! {
use std::io;
#[cfg(not(target_os = "hermit"))]
use std::os::fd::RawFd;
// TODO: once <https://github.com/rust-lang/rust/issues/126198> is fixed this
// can use `std::os::fd` and be merged with the above.
#[cfg(target_os = "hermit")]
use std::os::hermit::io::RawFd;
use std::sync::{Arc, Mutex};

use crate::sys::Selector;
use crate::{Interest, Registry, Token};
}

/// Shared record between IoSourceState and SelectorState that allows us to
/// internally deregister partially or fully closed fds (i.e. when we get
/// POLLHUP or PULLERR) without confusing IoSourceState and trying to deregister
/// twice.  This isn't strictly required as technically deregister is idempotent
/// but it is confusing when trying to debug behaviour as we get imbalanced
/// calls to register/deregister and superfluous NotFound errors.
#[derive(Debug)]
pub(crate) struct RegistrationRecord {
    is_unregistered: AtomicBool,
}

impl RegistrationRecord {
    pub(crate) fn new() -> RegistrationRecord {
        RegistrationRecord {
            is_unregistered: AtomicBool::new(false),
        }
    }

    pub(crate) fn mark_unregistered(&self) {
        self.is_unregistered.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_registered(&self) -> bool {
        !self.is_unregistered.load(Ordering::Relaxed)
    }
}

cfg_io_source! {
pub(crate) struct IoSourceState {
    inner: Option<Box<InternalState>>,
}

struct InternalState {
    selector: Selector,
    token: Token,
    interests: Interest,
    fd: RawFd,
    shared_record: Mutex<Arc<RegistrationRecord>>,
}
}

cfg_io_source! {
impl IoSourceState {
    pub(crate) fn new() -> IoSourceState {
        IoSourceState { inner: None }
    }

    pub(crate) fn do_io<T, F, R>(&self, f: F, io: &T) -> io::Result<R>
    where
        F: FnOnce(&T) -> io::Result<R>,
    {
        let result = f(io);

        if let Err(err) = &result {
            if err.kind() == io::ErrorKind::WouldBlock {
                self.inner.as_ref().map_or(Ok(()), |state| {
                    state.rearm(&state.selector, state.fd, state.token, state.interests)
                })?;
            }
        }

        result
    }

    pub(crate) fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
        fd: RawFd,
    ) -> io::Result<()> {
        if self.inner.is_some() {
            Err(io::ErrorKind::AlreadyExists.into())
        } else {
            let selector = registry.selector().try_clone()?;

            selector
                .register_internal(fd, token, interests)
                .map(move |shared_record| {
                    let state = InternalState {
                        selector,
                        token,
                        interests,
                        fd,
                        shared_record: Mutex::new(shared_record),
                    };

                    self.inner = Some(Box::new(state));
                })
        }
    }

    pub(crate) fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
        fd: RawFd,
    ) -> io::Result<()> {
        match self.inner.as_mut() {
            Some(state) => state
                .rearm(registry.selector(), fd, token, interests)
                .map(|()| {
                    state.token = token;
                    state.interests = interests;
                }),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }

    pub(crate) fn deregister(&mut self, registry: &Registry, fd: RawFd) -> io::Result<()> {
        let Some(mut state) = self.inner.take() else {
            return Err(io::ErrorKind::NotFound.into());
        };
        let result = registry.selector().deregister(fd);
        let record = state.shared_record.get_mut().unwrap_or_else(|err| err.into_inner());
        let internally_removed = !record.is_registered();
        // The explicit selector call performed cleanup; Drop must not
        // deregister this source again.
        record.mark_unregistered();

        match result {
            Err(err) if internally_removed && err.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

impl InternalState {
    fn rearm(&self, selector: &Selector, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        // Serialize replacement of the shared record when concurrent I/O calls
        // re-arm a source removed internally by the selector.
        let mut record = self.shared_record.lock().unwrap_or_else(|err| err.into_inner());
        match selector.reregister(fd, token, interests) {
            Err(err) if err.kind() == io::ErrorKind::NotFound && !record.is_registered() => {
                *record = selector.register_internal(fd, token, interests)?;
                Ok(())
            }
            result => result,
        }
    }
}

impl Drop for InternalState {
    fn drop(&mut self) {
        if self.shared_record.get_mut().unwrap_or_else(|err| err.into_inner()).is_registered() {
            let _ = self.selector.deregister(self.fd);
        }
    }
}
}

#[cfg(all(test, mio_unsupported_force_poll_poll, feature = "net"))]
mod tests {
    use super::*;
    use crate::{Events, Poll};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    #[cfg(not(target_os = "hermit"))]
    use std::os::fd::AsRawFd;
    #[cfg(target_os = "hermit")]
    use std::os::hermit::io::AsRawFd;
    use std::time::Duration;

    #[test]
    fn would_block_rearms_after_internal_removal() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        stream.set_nonblocking(true).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        let mut poll = Poll::new().unwrap();
        let mut state = IoSourceState::new();
        let fd = stream.as_raw_fd();
        state
            .register(poll.registry(), Token(1), Interest::READABLE, fd)
            .unwrap();

        // Exercise the selector's internal-removal path without deregistering
        // the live source. HUP/ERR calls this same selector operation.
        poll.registry().selector().deregister(fd).unwrap();
        let mut buf = [0; 1];
        assert_eq!(
            state
                .do_io(|mut stream| stream.read(&mut buf), &stream)
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        peer.write_all(&[2]).unwrap();
        let mut events = Events::with_capacity(8);
        poll.poll(&mut events, Some(Duration::from_secs(1)))
            .unwrap();
        assert!(events
            .iter()
            .any(|event| event.token() == Token(1) && event.is_readable()));
        assert_eq!(
            state
                .do_io(|mut stream| stream.read(&mut buf), &stream)
                .unwrap(),
            1
        );
        assert_eq!(buf, [2]);

        // Explicit reregistration also replaces the removed record and token.
        poll.registry().selector().deregister(fd).unwrap();
        state
            .reregister(poll.registry(), Token(2), Interest::READABLE, fd)
            .unwrap();
        peer.write_all(&[3]).unwrap();
        poll.poll(&mut events, Some(Duration::from_secs(1)))
            .unwrap();
        assert!(events
            .iter()
            .any(|event| event.token() == Token(2) && event.is_readable()));
        assert_eq!(
            state
                .do_io(|mut stream| stream.read(&mut buf), &stream)
                .unwrap(),
            1
        );
        assert_eq!(buf, [3]);
        state.deregister(poll.registry(), fd).unwrap();
        assert_eq!(
            state.deregister(poll.registry(), fd).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
}
