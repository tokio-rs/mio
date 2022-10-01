use crate::{Interest, Registry, Token};
use std::io;
use std::os::unix::io::RawFd;

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

    pub fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
        fd: RawFd,
    ) -> io::Result<()> {
        // Pass through, we don't have any state
        registry.selector().register(fd, token, interests)
    }

    pub fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
        fd: RawFd,
    ) -> io::Result<()> {
        // Pass through, we don't have any state
        registry.selector().reregister(fd, token, interests)
    }

    pub fn deregister(&mut self, registry: &Registry, fd: RawFd) -> io::Result<()> {
        // Pass through, we don't have any state
        registry.selector().deregister(fd)
    }
}
