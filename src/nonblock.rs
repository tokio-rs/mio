use io::{self, Evented, TryRead, TryWrite, Result};
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::os::unix::{Fd, AsRawFd};

#[derive(Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct NonBlock<T> {
    inner: T,
}

impl<T> NonBlock<T> {
    pub fn new(val: T) -> NonBlock<T> {
        NonBlock { inner: val }
    }

    pub fn unwrap(self) -> T {
        self.inner
    }
}

impl<T> Deref for NonBlock<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> DerefMut for NonBlock<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: Read> TryRead for NonBlock<T> {
    fn read_slice(&mut self, buf: &mut [u8]) -> Result<Option<usize>> {
        (**self).read(buf)
            .map(|cnt| Some(cnt))
            .or_else(io::to_non_block)
    }
}

impl<T: Write> TryWrite for NonBlock<T> {
    fn write_slice(&mut self, buf: &[u8]) -> Result<Option<usize>> {
        (**self).write(buf)
            .map(|cnt| Some(cnt))
            .or_else(io::to_non_block)
    }
}

impl<T: AsRawFd> AsRawFd for NonBlock<T> {
    fn as_raw_fd(&self) -> Fd {
        (**self).as_raw_fd()
    }
}

impl<T: Evented> Evented for NonBlock<T> {
}

pub trait AsNonBlock {
    fn as_non_block(self) -> Result<NonBlock<Self>>;
}
