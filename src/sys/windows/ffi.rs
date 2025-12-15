

// Code from timer-deque-rs crate.

use std::
{
    io::{self}, 
    os::
    {
        raw::c_void, 
        windows::io::{FromRawHandle, OwnedHandle, RawHandle}
    }, 
    ptr::null_mut
};

use windows_sys::{Win32::Foundation::GENERIC_ALL, core::HRESULT};
pub use windows_sys::{Win32::Foundation::HANDLE};

/// A wrapper for raw NTSTATUS
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct NTSTATUS(pub i32);

impl NTSTATUS
{
    #[inline]
    pub 
    fn into_result(self) -> io::Result<()>
    {
        if self.0 >= 0
        {
            return Ok(());
        } 
        else 
        {
            return Err(io::Error::from_raw_os_error(self.0));
        }
    }

    #[inline]
    pub const fn into_hresult(self) -> HRESULT 
    {
        if self.0 >= 0 
        {
            return self.0;
        }
        else 
        {
            return self.0 | 0x1000_0000;
        }
    }
}

/// This is a dummy declaration.
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug)]
pub struct UNICODE_STRING 
{
    Length: u16,
    MaximumLength: u16,
    Buffer: *mut u16,
}

/// This is a dummy declaration.
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug)]
pub struct OBJECT_ATTRIBUTES 
{
    Length: u32,
    RootDirectory: RawHandle,
    ObjectName: *mut UNICODE_STRING,
    Attributes: u32,
    SecurityDescriptor: *mut c_void,
    SecurityQualityOfService: *mut c_void,
}

#[link(name = "ntdll")]
unsafe extern "system" 
{
    pub unsafe fn NtCreateWaitCompletionPacket(
        IoCompletionHandle: HANDLE, // out
        DesiredAccess: u32,
        ObjectAttributes: *mut OBJECT_ATTRIBUTES,
    ) -> NTSTATUS;

    pub unsafe fn NtAssociateWaitCompletionPacket(
        WaitCompletionPacketHandle: HANDLE,
        IoCompletionHandle: HANDLE,
        TargetObjectHandle: HANDLE,
        KeyContext: *mut c_void,
        ApcContext: *mut c_void,
        IoStatus: i32,
        IoStatusInformation: usize,
        AlreadySignaled: *mut u8,
    ) -> NTSTATUS;

    pub unsafe fn NtCancelWaitCompletionPacket(
        WaitCompletionPacketHandle: RawHandle,
        RemoveSignaledPacket: u8, // boolean
    ) -> NTSTATUS;

    pub unsafe fn NtRemoveIoCompletion(
        IoCompletionHandle: HANDLE, 
        KeyContext: *mut c_void,
        ApcContext: *mut c_void,
        IoStatusBlock: *mut c_void,
        Timeout: *const i64,
    ) -> NTSTATUS;
}

/// Error for `NtCancelWaitCompletionPacket` indicating double cancell.
pub const IO_CANCELLED_ERROR: HRESULT = 0xD0000120 as u32 as i32;

/// A wrapper for the [NtCreateWaitCompletionPacket] `ntdll` call.
pub(crate) 
fn ffi_nt_create_wait_completion_packet() -> io::Result<OwnedHandle>
{
    let mut hps: *mut c_void = null_mut();

    // try to create a wait completion packet
    unsafe
    {
        NtCreateWaitCompletionPacket(
            &mut hps as *mut _ as *mut _, 
            GENERIC_ALL, 
            null_mut()
        )
        .into_result()?
    }; 

    return Ok(unsafe { OwnedHandle::from_raw_handle(hps) });
}