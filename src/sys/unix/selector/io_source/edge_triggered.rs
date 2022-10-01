use crate::sys::Selector;
use crate::{Interest, Registry, Token};
use std::io;
use std::os::unix::io::RawFd;
use std::sync::Arc;

struct InternalState {
    selector: Arc<Selector>,
    token: Token,
    interests: Interest,
    fd: RawFd,
    is_registered: bool,
}

impl Drop for InternalState {
    fn drop(&mut self) {
        if self.is_registered {
            let _ = self.selector.deregister(self.fd);
        }
    }
}

pub(crate) struct IoSourceState {
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

        if let Err(err) = &result {
            if err.kind() == io::ErrorKind::WouldBlock {
                self.inner.as_ref().map_or(Ok(()), |state| {
                    state
                        .selector
                        .reregister(state.fd, state.token, state.interests)
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
        fd: RawFd,
    ) -> io::Result<()> {
        if self.inner.is_some() {
            Err(io::ErrorKind::AlreadyExists.into())
        } else {
            registry.selector().register(fd, token, interests)
        }
    }

    pub fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
        fd: RawFd,
    ) -> io::Result<()> {
        match self.inner.as_mut() {
            Some(state) => registry
                .selector()
                .reregister(fd, token, interests)
                .map(|()| {
                    state.token = token;
                    state.interests = interests;
                }),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }

    pub fn deregister(&mut self, registry: &Registry, fd: RawFd) -> io::Result<()> {
        registry.selector().deregister(fd)
    }
}
