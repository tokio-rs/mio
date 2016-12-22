use {io, sys, Evented, Ready, Poll, PollOpt, Token};
use std::io::{Read, Write};
use std::process;

pub use sys::Io;

/*
 *
 * ===== Pipe =====
 *
 */

pub fn pipe() -> io::Result<(PipeReader, PipeWriter)> {
    let (rd, wr) = try!(sys::pipe());
    Ok((From::from(rd), From::from(wr)))
}

#[derive(Debug)]
pub struct PipeReader {
    io: Io,
}

impl PipeReader {
    pub fn from_stdout(stdout: process::ChildStdout) -> io::Result<Self> {
        match sys::set_nonblock(&stdout) {
            Err(e) => return Err(e),
            _ => {},
        }
        return Ok(PipeReader::from(unsafe { Io::from_raw_fd(stdout.into_raw_fd()) }));
    }
    pub fn from_stderr(stderr: process::ChildStderr) -> io::Result<Self> {
        match sys::set_nonblock(&stderr) {
            Err(e) => return Err(e),
            _ => {},
        }
        return Ok(PipeReader::from(unsafe { Io::from_raw_fd(stderr.into_raw_fd()) }));
    }
}

impl Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.read(buf)
    }
}

impl<'a> Read for &'a PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.io).read(buf)
    }
}

impl Evented for PipeReader {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.io.register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.io.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.io.deregister(poll)
    }
}

impl From<Io> for PipeReader {
    fn from(io: Io) -> PipeReader {
        PipeReader { io: io }
    }
}

#[derive(Debug)]
pub struct PipeWriter {
    io: Io,
}

impl PipeWriter {
    pub fn from_stdin(stdin: process::ChildStdin) -> io::Result<Self> {
        match sys::set_nonblock(&stdin) {
            Err(e) => return Err(e),
            _ => {},
        }
        return Ok(PipeWriter::from(unsafe { Io::from_raw_fd(stdin.into_raw_fd()) }));
    }
}

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.io.flush()
    }
}

impl<'a> Write for &'a PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.io).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.io).flush()
    }
}

impl Evented for PipeWriter {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.io.register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        self.io.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.io.deregister(poll)
    }
}

impl From<Io> for PipeWriter {
    fn from(io: Io) -> PipeWriter {
        PipeWriter { io: io }
    }
}

/*
 *
 * ===== Conversions =====
 *
 */

 use std::os::unix::io::{RawFd, IntoRawFd, AsRawFd, FromRawFd};

 impl IntoRawFd for PipeReader {
     fn into_raw_fd(self) -> RawFd {
         self.io.into_raw_fd()
     }
 }

 impl AsRawFd for PipeReader {
     fn as_raw_fd(&self) -> RawFd {
         self.io.as_raw_fd()
     }
 }

 impl FromRawFd for PipeReader {
     unsafe fn from_raw_fd(fd: RawFd) -> PipeReader {
         PipeReader { io: FromRawFd::from_raw_fd(fd) }
     }
 }

 impl IntoRawFd for PipeWriter {
     fn into_raw_fd(self) -> RawFd {
         self.io.into_raw_fd()
     }
 }

 impl AsRawFd for PipeWriter {
     fn as_raw_fd(&self) -> RawFd {
         self.io.as_raw_fd()
     }
 }

 impl FromRawFd for PipeWriter {
     unsafe fn from_raw_fd(fd: RawFd) -> PipeWriter {
         PipeWriter { io: FromRawFd::from_raw_fd(fd) }
     }
 }
