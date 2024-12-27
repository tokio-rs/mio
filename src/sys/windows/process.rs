use crate::event::Source;
use crate::sys::windows::iocp::CompletionPort;
use crate::{Interest, Registry, Token};
use std::collections::HashMap;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{
    AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, IntoRawHandle, OwnedHandle, RawHandle,
};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, LazyLock, Mutex};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectAssociateCompletionPortInformation,
    SetInformationJobObject, JOBOBJECT_ASSOCIATE_COMPLETION_PORT,
};

// Odd tokens are for named pipes
static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(2);
// inner token -> token
pub(crate) static TOKEN_MAPPING: LazyLock<Mutex<HashMap<usize, Token>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// https://devblogs.microsoft.com/oldnewthing/20130405-00/?p=4743
#[derive(Debug)]
pub struct Process {
    job: OwnedHandle,
    io: Mutex<Io>,
}

impl Process {
    pub fn new(child: RawHandle) -> io::Result<Self> {
        let job = new_job()?;
        assign_process_to_job(job.as_raw_handle(), child)?;
        let io = Mutex::new(Io {
            cp: None,
            token: None,
        });
        Ok(Self { job, io })
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
        let io = Mutex::new(Io {
            cp: None,
            token: None,
        });
        let job = OwnedHandle::from_raw_handle(job);
        Self { job, io }
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
        let io = Mutex::new(Io {
            cp: None,
            token: None,
        });
        Self { job, io }
    }
}

impl Source for Process {
    fn register(&mut self, registry: &Registry, token: Token, _: Interest) -> io::Result<()> {
        let mut io = self.io.lock().unwrap();

        io.check_association(registry, false)?;

        if io.token.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "I/O source already registered with a `Registry`",
            ));
        }

        if io.cp.is_none() {
            let selector = registry.selector();

            let port = selector.clone_port();

            let inner_token = NEXT_TOKEN.fetch_add(3, Relaxed) + 3;
            TOKEN_MAPPING.lock().unwrap().insert(inner_token, token);
            associate_job_with_completion_port(
                self.job.as_raw_handle(),
                port.as_raw_handle(),
                inner_token,
            )?;

            io.cp = Some(port);
        }

        io.token = Some(token);
        drop(io);

        Ok(())
    }

    fn reregister(&mut self, registry: &Registry, token: Token, _: Interest) -> io::Result<()> {
        let mut io = self.io.lock().unwrap();

        io.check_association(registry, true)?;

        io.token = Some(token);
        drop(io);

        Ok(())
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        let mut io = self.io.lock().unwrap();

        io.check_association(registry, true)?;

        if io.token.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "I/O source not registered with `Registry`",
            ));
        }

        io.token = None;
        Ok(())
    }
}

#[derive(Debug)]
struct Io {
    // Uniquely identifies the selector associated with this named pipe
    cp: Option<Arc<CompletionPort>>,
    // Token used to identify events
    token: Option<Token>,
}

impl Io {
    fn check_association(&self, registry: &Registry, required: bool) -> io::Result<()> {
        match self.cp {
            Some(ref cp) if !registry.selector().same_port(cp) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "I/O source already registered with a different `Registry`",
            )),
            None if required => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "I/O source not registered with `Registry`",
            )),
            _ => Ok(()),
        }
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
