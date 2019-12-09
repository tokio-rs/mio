use lazy_static::lazy_static;
use ntapi::ntioapi::{IO_STATUS_BLOCK_u, IO_STATUS_BLOCK};
use ntapi::ntioapi::{NtCancelIoFileEx, NtDeviceIoControlFile};
use ntapi::ntrtl::RtlNtStatusToDosError;
use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::ptr::null_mut;
use winapi::shared::ntdef::{
    HANDLE, LARGE_INTEGER, NTSTATUS, OBJECT_ATTRIBUTES, PVOID, ULONG, UNICODE_STRING,
};
use winapi::shared::ntstatus::{STATUS_NOT_FOUND, STATUS_PENDING, STATUS_SUCCESS};

const IOCTL_AFD_POLL: ULONG = 0x00012024;

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

/// Winsock2 AFD driver instance.
///
/// All operations are unsafe due to IO_STATUS_BLOCK parameter are being used by Afd driver during STATUS_PENDING before I/O Completion Port returns its result.
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

impl fmt::Debug for AfdPollInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AfdPollInfo").finish()
    }
}

impl Afd {
    /// Poll `Afd` instance with `AfdPollInfo`.
    ///
    /// # Unsafety
    ///
    /// This function is unsafe due to memory of `IO_STATUS_BLOCK` still being used by `Afd` instance while `Ok(false)` (`STATUS_PENDING`).
    /// `iosb` needs to be untouched after the call while operation is in effective at ALL TIME except for `cancel` method.
    /// So be careful not to `poll` twice while polling.
    /// User should deallocate there overlapped value when error to prevent memory leak.
    pub unsafe fn poll(
        &self,
        info: &mut AfdPollInfo,
        iosb: *mut IO_STATUS_BLOCK,
        overlapped: PVOID,
    ) -> io::Result<bool> {
        let info_ptr: PVOID = info as *mut _ as PVOID;
        (*iosb).u.Status = STATUS_PENDING;
        let status = NtDeviceIoControlFile(
            self.fd.as_raw_handle(),
            null_mut(),
            None,
            overlapped,
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

    /// Cancel previous polled request of `Afd`.
    ///
    /// iosb needs to be used by `poll` first for valid `cancel`.
    ///
    /// # Unsafety
    ///
    /// This function is unsafe due to memory of `IO_STATUS_BLOCK` still being used by `Afd` instance while `Ok(false)` (`STATUS_PENDING`).
    /// Use it only with request is still being polled so that you have valid `IO_STATUS_BLOCK` to use.
    /// User should NOT deallocate there overlapped value after the `cancel` to prevent double free.
    pub unsafe fn cancel(&self, iosb: *mut IO_STATUS_BLOCK) -> io::Result<()> {
        if (*iosb).u.Status != STATUS_PENDING {
            return Ok(());
        }

        let mut cancel_iosb = IO_STATUS_BLOCK {
            u: IO_STATUS_BLOCK_u { Status: 0 },
            Information: 0,
        };
        let status = NtCancelIoFileEx(self.fd.as_raw_handle(), iosb, &mut cancel_iosb);
        if status == STATUS_SUCCESS || status == STATUS_NOT_FOUND {
            return Ok(());
        }
        Err(io::Error::from_raw_os_error(
            RtlNtStatusToDosError(status) as i32
        ))
    }
}

cfg_net! {
    use miow::iocp::CompletionPort;
    use ntapi::ntioapi::FILE_OPEN;
    use ntapi::ntioapi::NtCreateFile;
    use std::mem::zeroed;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use winapi::um::handleapi::INVALID_HANDLE_VALUE;
    use winapi::um::winbase::{SetFileCompletionNotificationModes, FILE_SKIP_SET_EVENT_ON_HANDLE};
    use winapi::um::winnt::SYNCHRONIZE;
    use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE};

    static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(0);

    impl AfdPollInfo {
        pub fn zeroed() -> AfdPollInfo {
            unsafe { zeroed() }
        }
    }

    impl Afd {
        /// Create new Afd instance.
        pub fn new(cp: &CompletionPort) -> io::Result<Afd> {
            let mut afd_helper_handle: HANDLE = INVALID_HANDLE_VALUE;
            let mut iosb = IO_STATUS_BLOCK {
                u: IO_STATUS_BLOCK_u { Status: 0 },
                Information: 0,
            };

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
                let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed) + 1;
                let afd = Afd { fd };
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
    }
}

pub const POLL_RECEIVE: u32 = 0b000_000_001;
pub const POLL_RECEIVE_EXPEDITED: u32 = 0b000_000_010;
pub const POLL_SEND: u32 = 0b000_000_100;
pub const POLL_DISCONNECT: u32 = 0b000_001_000;
pub const POLL_ABORT: u32 = 0b000_010_000;
pub const POLL_LOCAL_CLOSE: u32 = 0b000_100_000;
// Not used as it indicated in each event where a connection is connected, not
// just the first time a connection is established.
// Also see https://github.com/piscisaureus/wepoll/commit/8b7b340610f88af3d83f40fb728e7b850b090ece.
pub const POLL_CONNECT: u32 = 0b001_000_000;
pub const POLL_ACCEPT: u32 = 0b010_000_000;
pub const POLL_CONNECT_FAIL: u32 = 0b100_000_000;

pub const KNOWN_EVENTS: u32 = POLL_RECEIVE
    | POLL_RECEIVE_EXPEDITED
    | POLL_SEND
    | POLL_DISCONNECT
    | POLL_ABORT
    | POLL_LOCAL_CLOSE
    | POLL_ACCEPT
    | POLL_CONNECT_FAIL;
