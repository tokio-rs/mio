use crate::event::Source;
use crate::{sys, Interest, Registry, Token};
use std::io::Error;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, RawHandle};
use std::process::Child;

/// Process allows polling OS processes for completion.
///
/// When the process exits the event with [`readable`](crate::event::Event::readable) readiness is generated.
///
/// # Notes
///
/// Events are delivered even if the process has exited by the time [`poll`](crate::Poll::poll) is
/// called and has been waited for.
///
/// # Implementation notes
///
/// [`Process`] uses `pidfd` on Linux, `EVFILT_PROC` on MacOS/BSD and `AssignProcessToJobObject` on
/// Windows.
#[derive(Debug)]
pub struct Process {
    inner: sys::Process,
}

impl Process {
    /// Create new process from [`Child`](std::process::Child).
    pub fn new(child: &Child) -> Result<Self, Error> {
        let inner = sys::Process::new(child.as_raw_handle())?;
        Ok(Self { inner })
    }

    /// Create new process from the process id.
    #[cfg(unix)]
    pub fn from_pid(pid: libc::pid_t) -> Result<Self, Error> {
        let inner = sys::Process::from_pid(pid)?;
        Ok(Self { inner })
    }

    /// Create new process from the process handle.
    #[cfg(windows)]
    pub fn from_handle(child: RawHandle) -> Result<Self, Error> {
        let inner = sys::Process::new(child)?;
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

// The following trait implementations are useful to send/receive `pidfd` over a UNIX-domain socket.
#[cfg(any(
    target_os = "android",
    target_os = "espidf",
    target_os = "fuchsia",
    target_os = "hermit",
    target_os = "illumos",
    target_os = "linux",
))]
mod linux {
    use super::*;
    #[cfg(not(target_os = "hermit"))]
    use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
    // TODO: once <https://github.com/rust-lang/rust/issues/126198> is fixed this
    // can use `std::os::fd` and be merged with the above.
    #[cfg(target_os = "hermit")]
    use std::os::hermit::io::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};

    impl AsFd for Process {
        fn as_fd(&self) -> BorrowedFd<'_> {
            self.inner.as_fd()
        }
    }

    impl AsRawFd for Process {
        fn as_raw_fd(&self) -> RawFd {
            self.inner.as_raw_fd()
        }
    }

    impl FromRawFd for Process {
        unsafe fn from_raw_fd(fd: RawFd) -> Self {
            let inner = sys::Process::from_raw_fd(fd);
            Self { inner }
        }
    }

    impl IntoRawFd for Process {
        fn into_raw_fd(self) -> RawFd {
            self.inner.into_raw_fd()
        }
    }

    impl From<OwnedFd> for Process {
        fn from(other: OwnedFd) -> Self {
            let inner = other.into();
            Self { inner }
        }
    }

    impl From<Process> for OwnedFd {
        fn from(other: Process) -> Self {
            other.inner.into()
        }
    }
}

#[cfg(windows)]
#[cfg_attr(docsrs, doc(cfg(windows)))]
mod windows {
    use super::*;
    use std::os::windows::io::{
        AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, IntoRawHandle, OwnedHandle, RawHandle,
    };

    impl AsRawHandle for Process {
        fn as_raw_handle(&self) -> RawHandle {
            self.inner.as_raw_handle()
        }
    }

    impl AsHandle for Process {
        fn as_handle(&self) -> BorrowedHandle<'_> {
            self.inner.as_handle()
        }
    }

    impl FromRawHandle for Process {
        unsafe fn from_raw_handle(job: RawHandle) -> Self {
            let inner = sys::Process::from_raw_handle(job);
            Self { inner }
        }
    }

    impl IntoRawHandle for Process {
        fn into_raw_handle(self) -> RawHandle {
            self.inner.into_raw_handle()
        }
    }

    impl From<Process> for OwnedHandle {
        fn from(other: Process) -> Self {
            other.inner.into()
        }
    }

    impl From<OwnedHandle> for Process {
        fn from(other: OwnedHandle) -> Self {
            let inner = other.into();
            Self { inner }
        }
    }
}
