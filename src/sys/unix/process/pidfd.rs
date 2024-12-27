use std::fs::File;
use std::io::Error;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::process::Child;

use libc::{pid_t, SYS_pidfd_open, PIDFD_NONBLOCK};

use crate::event::Source;
use crate::{Interest, Registry, Token};

#[derive(Debug)]
pub struct Process {
    fd: File,
}

impl Process {
    pub fn new(child: &Child) -> Result<Self, Error> {
        Self::from_pid(child.id() as pid_t)
    }

    pub fn from_pid(pid: pid_t) -> Result<Self, Error> {
        let fd = syscall!(syscall(SYS_pidfd_open, pid, PIDFD_NONBLOCK))?;
        // SAFETY: `pidfd_open(2)` ensures the fd is valid.
        let fd = unsafe { File::from_raw_fd(fd as RawFd) };
        Ok(Self { fd })
    }
}

impl AsRawFd for Process {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl FromRawFd for Process {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        let fd = File::from_raw_fd(fd);
        Self { fd }
    }
}

impl IntoRawFd for Process {
    fn into_raw_fd(self) -> RawFd {
        self.fd.into_raw_fd()
    }
}

impl Source for Process {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> Result<(), Error> {
        registry
            .selector()
            .register(self.as_raw_fd(), token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> Result<(), Error> {
        registry
            .selector()
            .reregister(self.as_raw_fd(), token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> Result<(), Error> {
        registry.selector().deregister(self.as_raw_fd())
    }
}
