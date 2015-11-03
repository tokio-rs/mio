use {io, Evented, EventSet, Io, PollOpt, Selector, Token};
use std::io::{Read, Write};
use std::os::unix::io::RawFd;

pub struct FileStream(Io);

pub fn stdin() -> FileStream {
    FileStream(Io::from_raw_fd(0))
}

pub fn stdout() -> FileStream {
    FileStream(Io::from_raw_fd(1))
}

pub fn stderr() -> FileStream {
    FileStream(Io::from_raw_fd(2))
}

impl FileStream {
    pub unsafe fn from_raw_fd(fd: RawFd) -> Self {
        FileStream(Io::from_raw_fd(fd))
    }
}

impl Read for FileStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for FileStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl Evented for FileStream {
    fn register(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.0.register(selector, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
        self.0.reregister(selector, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
        self.0.deregister(selector)
    }
}
