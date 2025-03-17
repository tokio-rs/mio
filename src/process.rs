use crate::event::Source;
use crate::{sys, Interest, Registry, Token};
use std::io::Error;
use std::process::Child;

/// Process allows polling OS processes for completion.
///
/// When the process exits the event with _readable_ readiness is generated.
///
/// # Notes
///
/// Events are delivered even if the process has exited and has been waited for
/// by the time [`poll`](crate::Poll::poll) is called.
///
/// # Implementation notes
///
/// [`Process`] uses `pidfd` on Linux, `kqueue`'s `EVFILT_PROC` on MacOS/BSD and
/// `AssignProcessToJobObject` on Windows.
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

// The following trait implementations are useful to send/receive `pidfd` over a UNIX-domain socket.
cfg_os_proc_pidfd! {
    use_fd_traits!();

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

cfg_os_proc_kqueue! {
    impl Process {
        /// Get process id.
        pub fn pid(&self) -> libc::pid_t {
            self.inner.pid()
        }
    }
}

cfg_os_proc_job_object! {
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
