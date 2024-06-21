use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};

use crate::sys::unix::pipe;

/// Waker backed by a unix pipe.
///
/// Waker controls both the sending and receiving ends and empties the pipe
/// if writing to it (waking) fails.
#[derive(Debug)]
pub(crate) struct Waker {
    sender: File,
    receiver: File,
}

impl Waker {
    pub(crate) fn new() -> io::Result<Waker> {
        let [receiver, sender] = pipe::new_raw()?;
        let sender = unsafe { File::from_raw_fd(sender) };
        let receiver = unsafe { File::from_raw_fd(receiver) };
        Ok(Waker { sender, receiver })
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        // The epoll emulation on some illumos systems currently requires
        // the pipe buffer to be completely empty for an edge-triggered
        // wakeup on the pipe read side.
        #[cfg(target_os = "illumos")]
        self.empty();

        match (&self.sender).write(&[1]) {
            Ok(_) => Ok(()),
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                // The reading end is full so we'll empty the buffer and try
                // again.
                self.empty();
                self.wake()
            }
            Err(ref err) if err.kind() == io::ErrorKind::Interrupted => self.wake(),
            Err(err) => Err(err),
        }
    }

    #[cfg(any(
        mio_unsupported_force_poll_poll,
        target_os = "espidf",
        target_os = "haiku",
        target_os = "nto",
        target_os = "solaris",
        target_os = "vita",
    ))]
    pub(crate) fn ack_and_reset(&self) {
        self.empty();
    }

    /// Empty the pipe's buffer, only need to call this if `wake` fails.
    /// This ignores any errors.
    fn empty(&self) {
        let mut buf = [0; 4096];
        loop {
            match (&self.receiver).read(&mut buf) {
                Ok(n) if n > 0 => continue,
                _ => return,
            }
        }
    }
}

impl AsRawFd for Waker {
    fn as_raw_fd(&self) -> RawFd {
        self.receiver.as_raw_fd()
    }
}
