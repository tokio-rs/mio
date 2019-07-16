use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io;
use std::mem::{size_of, transmute};
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
const AFD_HELPER_NAME: &'static str = "\\Device\\Afd\\Mio";

static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(0);

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
unsafe impl Sync for AfdPollHandleInfo {}

#[repr(C)]
pub struct AfdPollInfo {
    pub timeout: LARGE_INTEGER,
    // Can have only value 1.
    pub number_of_handles: ULONG,
    pub exclusive: ULONG,
    pub handles: [AfdPollHandleInfo; 1],
}

impl fmt::Debug for AfdPollInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AfdPollInfo")
    }
}

impl Afd {
    pub fn new(cp: &CompletionPort) -> io::Result<Afd> {
        let mut afd_helper_name = OsStr::new(AFD_HELPER_NAME)
            .encode_wide()
            .collect::<Vec<_>>();

        let mut afd_helper_handle: HANDLE = INVALID_HANDLE_VALUE;
        let mut iosb: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };

        unsafe {
            let mut objname = UNICODE_STRING {
                // Lengths are calced in bytes
                Length: (afd_helper_name.len() * 2) as u16,
                MaximumLength: (afd_helper_name.len() * 2) as u16,
                Buffer: afd_helper_name.as_mut_ptr(),
            };
            let mut afd_helper_attributes = OBJECT_ATTRIBUTES {
                Length: size_of::<OBJECT_ATTRIBUTES>() as ULONG,
                RootDirectory: null_mut() as HANDLE,
                ObjectName: &mut objname,
                Attributes: 0 as ULONG,
                SecurityDescriptor: null_mut() as PVOID,
                SecurityQualityOfService: null_mut() as PVOID,
            };
            let status = NtCreateFile(
                &mut afd_helper_handle,
                SYNCHRONIZE,
                &mut afd_helper_attributes,
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
        iosb: &mut IO_STATUS_BLOCK,
        apccontext: PVOID,
    ) -> io::Result<bool> {
        unsafe {
            let info_ptr: PVOID = transmute(info);
            iosb.u.Status = STATUS_PENDING;
            let status = NtDeviceIoControlFile(
                self.fd.as_raw_handle(),
                null_mut(),
                None,
                apccontext,
                iosb,
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

    pub fn cancel(&self, iosb: &mut IO_STATUS_BLOCK) -> io::Result<()> {
        unsafe {
            if iosb.u.Status != STATUS_PENDING {
                return Ok(());
            }

            let mut cancel_iosb: IO_STATUS_BLOCK = std::mem::zeroed();
            let status = NtCancelIoFileEx(self.fd.as_raw_handle(), iosb, &mut cancel_iosb);
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
