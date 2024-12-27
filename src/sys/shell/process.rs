use crate::event::Source;
use crate::{Interest, Registry, Token};
use libc::pid_t;
use std::io::Error;
use std::process::Child;

#[derive(Debug)]
pub struct Process {}

impl Process {
    pub fn new(_: &Child) -> Result<Self, Error> {
        os_required!()
    }

    #[cfg(unix)]
    pub fn from_pid(_: pid_t) -> Result<Self, Error> {
        os_required!()
    }
}

#[cfg(any(
    target_os = "android",
    target_os = "espidf",
    target_os = "fuchsia",
    target_os = "hermit",
    target_os = "illumos",
    target_os = "linux",
))]
mod linux {
    use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};

    use super::*;

    impl AsFd for Process {
        fn as_fd(&self) -> BorrowedFd<'_> {
            os_required!()
        }
    }

    impl AsRawFd for Process {
        fn as_raw_fd(&self) -> RawFd {
            os_required!()
        }
    }

    impl FromRawFd for Process {
        unsafe fn from_raw_fd(_: RawFd) -> Self {
            os_required!()
        }
    }

    impl IntoRawFd for Process {
        fn into_raw_fd(self) -> RawFd {
            os_required!()
        }
    }

    impl From<OwnedFd> for Process {
        fn from(_: OwnedFd) -> Self {
            os_required!()
        }
    }

    impl From<Process> for OwnedFd {
        fn from(_: Process) -> Self {
            os_required!()
        }
    }
}

impl Source for Process {
    fn register(&mut self, _: &Registry, _: Token, _: Interest) -> Result<(), Error> {
        os_required!()
    }

    fn reregister(&mut self, _: &Registry, _: Token, _: Interest) -> Result<(), Error> {
        os_required!()
    }

    fn deregister(&mut self, _: &Registry) -> Result<(), Error> {
        os_required!()
    }
}
