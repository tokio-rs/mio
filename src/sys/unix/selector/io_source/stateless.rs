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
