use crate::event::Source;
use crate::sys::windows::selector::HandleInfo;
use crate::{Interest, Registry, Token};
use std::io;
use std::mem::size_of;
use std::os::windows::io::{
    AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, IntoRawHandle, OwnedHandle, RawHandle,
};
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
    pub fn new(child: RawHandle) -> io::Result<Self> {
        let job = new_job()?;
        assign_process_to_job(job.as_raw_handle(), child)?;
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
        other.job.into()
    }
}

impl From<OwnedHandle> for Process {
    fn from(other: OwnedHandle) -> Self {
        let job = other.into();
        Self { job }
    }
}

impl Source for Process {
    fn register(&mut self, registry: &Registry, token: Token, _: Interest) -> io::Result<()> {
        let selector = registry.selector();
        let port = selector.as_raw_handle();
        let handle = self.job.as_raw_handle();
        let info = HandleInfo::Process(token);
        selector.register_handle(handle, info)?;
        associate_job_with_completion_port(self.job.as_raw_handle(), port, handle as usize)?;
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

fn new_job() -> io::Result<OwnedHandle> {
    // SAFETY: `CreateJobObjectW` returns null pointer on failure and a valid job handle otherwise.
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `CreateJobObjectW` returns a valid handle on success.
    let job = unsafe { OwnedHandle::from_raw_handle(handle as _) };
    Ok(job)
}

fn assign_process_to_job(job: RawHandle, child: RawHandle) -> io::Result<()> {
    if unsafe { AssignProcessToJobObject(job as _, child as _) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn associate_job_with_completion_port(
    job: RawHandle,
    port: RawHandle,
    key: usize,
) -> io::Result<()> {
    let job_port = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
        CompletionKey: key as _,
        CompletionPort: port as _,
    };
    // SAFETY: We provide valid `job` and `port` handles.
    if unsafe {
        SetInformationJobObject(
            job as _,
            JobObjectAssociateCompletionPortInformation,
            std::ptr::from_ref(&job_port) as _,
            size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
