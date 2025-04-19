use crate::event::Source;
use crate::sys::windows::{AsHandlePtr, HandleInfo};
use crate::{Interest, Registry, Token};
use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{
    AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, IntoRawHandle, OwnedHandle, RawHandle,
};
use std::process::Child;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectAssociateCompletionPortInformation,
    SetInformationJobObject, JOBOBJECT_ASSOCIATE_COMPLETION_PORT,
};

// https://devblogs.microsoft.com/oldnewthing/20130405-00/?p=4743
#[derive(Debug)]
pub struct Process {
    job: OwnedHandle,
}

impl Process {
    pub fn new(child: &Child) -> io::Result<Self> {
        // SAFETY: `CreateJobObjectW` returns null pointer on failure and a valid job handle otherwise.
        let job = syscall!(
            CreateJobObjectW(std::ptr::null(), std::ptr::null()),
            PartialEq::eq,
            0
        )?;
        // SAFETY: `CreateJobObjectW` returns a valid handle on success.
        let job = unsafe { OwnedHandle::from_raw_handle(job as RawHandle) };
        // SAFETY: We provide valid `job` and `child` handles.
        syscall!(
            AssignProcessToJobObject(job.as_handle_ptr(), child.as_handle_ptr()),
            PartialEq::eq,
            0
        )?;
        Ok(Self { job })
    }
}

impl AsRawHandle for Process {
    fn as_raw_handle(&self) -> RawHandle {
        self.job.as_raw_handle()
    }
}

impl AsHandle for Process {
    fn as_handle(&self) -> BorrowedHandle<'_> {
        self.job.as_handle()
    }
}

impl FromRawHandle for Process {
    unsafe fn from_raw_handle(job: RawHandle) -> Self {
        let job = OwnedHandle::from_raw_handle(job);
        Self { job }
    }
}

impl IntoRawHandle for Process {
    fn into_raw_handle(self) -> RawHandle {
        self.job.into_raw_handle()
    }
}

impl From<Process> for OwnedHandle {
    fn from(other: Process) -> Self {
        other.job
    }
}

impl From<OwnedHandle> for Process {
    fn from(job: OwnedHandle) -> Self {
        Self { job }
    }
}

impl Source for Process {
    fn register(&mut self, registry: &Registry, token: Token, _: Interest) -> io::Result<()> {
        let selector = registry.selector();
        let handle = self.job.as_raw_handle();
        let info = HandleInfo::Process(token);
        selector.register_handle(handle, info)?;
        let job_port = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
            CompletionKey: self.job.as_raw_handle(),
            CompletionPort: selector.as_raw_handle() as HANDLE,
        };
        // SAFETY: We provide valid `job` and `port` handles.
        syscall!(
            SetInformationJobObject(
                self.job.as_handle_ptr(),
                JobObjectAssociateCompletionPortInformation,
                &job_port as *const JOBOBJECT_ASSOCIATE_COMPLETION_PORT as *const c_void,
                size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
            ),
            PartialEq::eq,
            0
        )?;
        Ok(())
    }

    fn reregister(&mut self, registry: &Registry, token: Token, _: Interest) -> io::Result<()> {
        let handle = self.job.as_raw_handle();
        let info = HandleInfo::Process(token);
        registry.selector().reregister_handle(handle, info)?;
        Ok(())
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        let handle = self.job.as_raw_handle();
        registry.selector().deregister_handle(handle)?;
        Ok(())
    }
}
