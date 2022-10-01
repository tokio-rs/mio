use crate::sys::Selector;
use crate::{Interest, Registry, Token};
use std::io;
use std::os::unix::io::RawFd;

struct InternalState {
    selector: Selector,
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
            let selector = registry.selector().try_clone()?;

            selector.register(fd, token, interests).map(move |()| {
                let state = InternalState {
                    selector,
                    token,
                    interests,
                    fd,
                    is_registered: false,
                };

                self.inner = Some(Box::new(state));
            })
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
        if let Some(mut state) = self.inner.take() {
            // Deregistration _may_ fail below, however, dropping the state would only
            // do the same thing twice anyway
            state.is_registered = false;
        }

        registry.selector().deregister(fd)
    }
}
