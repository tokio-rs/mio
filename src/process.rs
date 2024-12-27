use std::io::Error;
#[cfg(any(
    target_os = "android",
    target_os = "espidf",
    target_os = "fuchsia",
    target_os = "hermit",
    target_os = "illumos",
    target_os = "linux",
))]
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::process::Child;

use crate::event::Source;
use crate::{sys, Interest, Registry, Token};

/// Process allows polling OS processes for completion.
///
/// When the process exits the event with [`readable`] readiness is generated.
///
/// # Notes
///
/// Events are delivered even if the process has exited by the time [`poll`](crate::Poll::poll) is
/// called.
///
/// # Implementation notes
///
/// On Linux `Process` uses `pidfd`.
#[derive(Debug)]
pub struct Process {
    inner: sys::Process,
}

impl Process {
    /// Create new process from [`Child`](std::process::Child).
    pub fn new(child: &Child) -> Result<Self, Error> {
        let inner = sys::Process::new(child)?;
        Ok(Self { inner })
    }

    /// Create new process from the process id.
    #[cfg(unix)]
    pub fn from_pid(pid: libc::pid_t) -> Result<Self, Error> {
        let inner = sys::Process::from_pid(pid)?;
        Ok(Self { inner })
    }
}

impl Source for Process {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> Result<(), Error> {
        self.inner.register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> Result<(), Error> {
        self.inner.reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> Result<(), Error> {
        self.inner.deregister(registry)
    }
}

// This `impl` is useful to send/receive `pidfd` over a UNIX domain socket.
#[cfg(any(
    target_os = "android",
    target_os = "espidf",
    target_os = "fuchsia",
    target_os = "hermit",
    target_os = "illumos",
    target_os = "linux",
))]
impl AsRawFd for Process {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

// This `impl` is useful to send/receive `pidfd` over a UNIX domain socket.
#[cfg(any(
    target_os = "android",
    target_os = "espidf",
    target_os = "fuchsia",
    target_os = "hermit",
    target_os = "illumos",
    target_os = "linux",
))]
impl FromRawFd for Process {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        let inner = sys::Process::from_raw_fd(fd);
        Self { inner }
    }
}

// This `impl` is useful to send/receive `pidfd` over a UNIX domain socket.
#[cfg(any(
    target_os = "android",
    target_os = "espidf",
    target_os = "fuchsia",
    target_os = "hermit",
    target_os = "illumos",
    target_os = "linux",
))]
impl IntoRawFd for Process {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}
