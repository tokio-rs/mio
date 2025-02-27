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

cfg_os_proc_pidfd! {
    use_fd_traits!();

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

cfg_os_proc_kqueue! {
    impl Process {
        pub fn pid(&self) -> libc::pid_t {
            os_required!()
        }
    }
}

cfg_os_proc_job_object! {
    use std::os::windows::io::{
        AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, IntoRawHandle, OwnedHandle, RawHandle,
    };

    impl AsRawHandle for Process {
        fn as_raw_handle(&self) -> RawHandle {
            os_required!()
        }
    }

    impl AsHandle for Process {
        fn as_handle(&self) -> BorrowedHandle<'_> {
            os_required!()
        }
    }

    impl FromRawHandle for Process {
        unsafe fn from_raw_handle(_: RawHandle) -> Self {
            os_required!()
        }
    }

    impl IntoRawHandle for Process {
        fn into_raw_handle(self) -> RawHandle {
            os_required!()
        }
    }

    impl From<Process> for OwnedHandle {
        fn from(_: Process) -> Self {
            os_required!()
        }
    }

    impl From<OwnedHandle> for Process {
        fn from(_: OwnedHandle) -> Self {
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
