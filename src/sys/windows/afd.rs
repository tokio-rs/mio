use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io;
use std::mem::{size_of, zeroed};
use std::pin::Pin;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicUsize, Ordering};

use miow::iocp::CompletionPort;

use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};

use winapi::shared::ntdef::{
    HANDLE, LARGE_INTEGER, NTSTATUS, OBJECT_ATTRIBUTES, PVOID, ULONG, UNICODE_STRING,
};
use winapi::shared::ntstatus::{STATUS_NOT_FOUND, STATUS_PENDING, STATUS_SUCCESS};
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::winbase::{SetFileCompletionNotificationModes, FILE_SKIP_SET_EVENT_ON_HANDLE};
use winapi::um::winnt::SYNCHRONIZE;
use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE};

use ntapi::ntioapi::FILE_OPEN;
use ntapi::ntioapi::IO_STATUS_BLOCK;
use ntapi::ntioapi::{NtCancelIoFileEx, NtCreateFile, NtDeviceIoControlFile};
use ntapi::ntrtl::RtlNtStatusToDosError;

const IOCTL_AFD_POLL: ULONG = 0x00012024;

static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    static ref AFD_HELPER_NAME: Vec<u16> = {
        OsStr::new("\\Device\\Afd\\Mio")
            .encode_wide()
            .collect::<Vec<_>>()
    };
}

struct UnicodeString(UNICODE_STRING);
unsafe impl Send for UnicodeString {}
unsafe impl Sync for UnicodeString {}

struct ObjectAttributes(OBJECT_ATTRIBUTES);
unsafe impl Send for ObjectAttributes {}
unsafe impl Sync for ObjectAttributes {}

lazy_static! {
    static ref AFD_OBJ_NAME: UnicodeString = UnicodeString(UNICODE_STRING {
        // Lengths are calced in bytes
        Length: (AFD_HELPER_NAME.len() * 2) as u16,
        MaximumLength: (AFD_HELPER_NAME.len() * 2) as u16,
        Buffer: AFD_HELPER_NAME.as_ptr() as *mut _,
    });
    static ref AFD_HELPER_ATTRIBUTES: ObjectAttributes = ObjectAttributes(OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as ULONG,
        RootDirectory: null_mut() as HANDLE,
        ObjectName: &AFD_OBJ_NAME.0 as *const _ as *mut _,
        Attributes: 0 as ULONG,
        SecurityDescriptor: null_mut() as PVOID,
        SecurityQualityOfService: null_mut() as PVOID,
    });
}

#[derive(Debug)]
pub struct Afd {
    fd: File,
}

#[repr(C)]
#[derive(Debug)]
pub struct AfdPollHandleInfo {
    pub handle: HANDLE,
    pub events: ULONG,
    pub status: NTSTATUS,
}

unsafe impl Send for AfdPollHandleInfo {}

#[repr(C)]
pub struct AfdPollInfo {
    pub timeout: LARGE_INTEGER,
    // Can have only value 1.
    pub number_of_handles: ULONG,
    pub exclusive: ULONG,
    pub handles: [AfdPollHandleInfo; 1],
}

impl AfdPollInfo {
    pub fn zeroed() -> AfdPollInfo {
        unsafe { zeroed::<AfdPollInfo>() }
    }
}

impl fmt::Debug for AfdPollInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AfdPollInfo").finish()
    }
}

impl Afd {
    pub fn new(cp: &CompletionPort) -> io::Result<Afd> {
        let mut afd_helper_handle: HANDLE = INVALID_HANDLE_VALUE;
        let mut iosb: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };

        unsafe {
            let status = NtCreateFile(
                &mut afd_helper_handle as *mut _,
                SYNCHRONIZE,
                &AFD_HELPER_ATTRIBUTES.0 as *const _ as *mut _,
                &mut iosb,
                null_mut(),
                0 as ULONG,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                FILE_OPEN,
                0 as ULONG,
                null_mut(),
                0 as ULONG,
            );
            if status != STATUS_SUCCESS {
                return Err(io::Error::from_raw_os_error(
                    RtlNtStatusToDosError(status) as i32
                ));
            }
            let fd = File::from_raw_handle(afd_helper_handle as RawHandle);
            let afd = Afd { fd };
            let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed) + 1;
            cp.add_handle(token, &afd.fd)?;
            match SetFileCompletionNotificationModes(
                afd_helper_handle,
                FILE_SKIP_SET_EVENT_ON_HANDLE,
            ) {
                0 => Err(io::Error::last_os_error()),
                _ => Ok(afd),
            }
        }
    }

    pub fn poll(
        &self,
        info: &mut AfdPollInfo,
        iosb: Pin<&mut IO_STATUS_BLOCK>,
        apccontext: PVOID,
    ) -> io::Result<bool> {
        let info_ptr: PVOID = info as *mut _ as PVOID;
        let iosb_ptr = iosb.get_mut();
        unsafe {
            (*iosb_ptr).u.Status = STATUS_PENDING;
            let status = NtDeviceIoControlFile(
                self.fd.as_raw_handle(),
                null_mut(),
                None,
                apccontext,
                iosb_ptr,
                IOCTL_AFD_POLL,
                info_ptr,
                size_of::<AfdPollInfo>() as u32,
                info_ptr,
                size_of::<AfdPollInfo>() as u32,
            );
            match status {
                STATUS_SUCCESS => Ok(true),
                STATUS_PENDING => Ok(false),
                _ => Err(io::Error::from_raw_os_error(
                    RtlNtStatusToDosError(status) as i32
                )),
            }
        }
    }

    pub fn cancel(&self, iosb: Pin<&mut IO_STATUS_BLOCK>) -> io::Result<()> {
        unsafe {
            if iosb.u.Status != STATUS_PENDING {
                return Ok(());
            }

            let mut cancel_iosb: IO_STATUS_BLOCK = std::mem::zeroed();
            let status =
                NtCancelIoFileEx(self.fd.as_raw_handle(), iosb.get_mut(), &mut cancel_iosb);
            if status == STATUS_SUCCESS || status == STATUS_NOT_FOUND {
                return Ok(());
            }
            Err(io::Error::from_raw_os_error(
                RtlNtStatusToDosError(status) as i32
            ))
        }
    }
}

pub const AFD_POLL_RECEIVE: u32 = 0x0001;
pub const AFD_POLL_RECEIVE_EXPEDITED: u32 = 0x0002;
pub const AFD_POLL_SEND: u32 = 0x0004;
pub const AFD_POLL_DISCONNECT: u32 = 0x0008;
pub const AFD_POLL_ABORT: u32 = 0x0010;
pub const AFD_POLL_LOCAL_CLOSE: u32 = 0x0020;
pub const AFD_POLL_ACCEPT: u32 = 0x0080;
pub const AFD_POLL_CONNECT_FAIL: u32 = 0x0100;
pub const KNOWN_AFD_EVENTS: u32 = AFD_POLL_RECEIVE
    | AFD_POLL_RECEIVE_EXPEDITED
    | AFD_POLL_SEND
    | AFD_POLL_DISCONNECT
    | AFD_POLL_ABORT
    | AFD_POLL_LOCAL_CLOSE
    | AFD_POLL_ACCEPT
    | AFD_POLL_CONNECT_FAIL;
