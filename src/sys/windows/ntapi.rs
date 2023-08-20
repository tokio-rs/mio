// The code in this file is adapted from the ntapi crate
// version 0.3.7 (https://crates.io/crates/ntapi/0.3.7)
// which was released under the MIT License or
// Apache License 2.0.
// This was necessary because parts of ntapi v.0.3.7
// uses code which is rejected in rust versions greater
// than or equal to version 1.68.
// See here for further information on the error:
// https://github.com/rust-lang/rust/issues/82523.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use winapi::shared::{
    basetsd::ULONG_PTR,
    ntdef::{HANDLE, NTSTATUS, PVOID, ULONG},
};

pub type PIO_STATUS_BLOCK = *mut IO_STATUS_BLOCK;

macro_rules! EXTERN {
    (extern $c:tt {$(
        fn $n:ident ($( $p:tt $(: $t:ty)?),* $(,)?) $(-> $r:ty)?;
    )+}) => {
        #[cfg_attr(all(target_env = "msvc", feature = "user"), link(name = "ntdll"))]
        #[cfg_attr(all(target_env = "msvc", feature = "kernel"), link(name = "ntoskrnl"))]
        extern $c {$(
            pub fn $n(
                $($p $(: $t)?),*
            ) $(-> $r)?;
        )+}
        $(
            #[cfg(feature = "func-types")]
            pub type $n = unsafe extern $c fn($($p $(: $t)?),*) $(-> $r)?;
        )+
    };
    (extern $c:tt {$(
        static mut $n:ident : $t:ty;
    )+}) => {
        #[cfg_attr(all(target_env = "msvc", feature = "user"), link(name = "ntdll"))]
        extern $c {$(
            pub static mut $n: $t;
        )+}
    };
}

macro_rules! FN {
    (stdcall $func:ident($($p:ident: $t:ty,)*) -> $ret:ty) => (
        pub type $func = Option<unsafe extern "system" fn($($p: $t,)*) -> $ret>;
    );
    (cdecl $func:ident($($p:ident: $t:ty,)*) -> $ret:ty) => (
        pub type $func = Option<unsafe extern "C" fn($($p: $t,)*) -> $ret>;
    );
}

macro_rules! STRUCT {
    (#[debug] $($rest:tt)*) => (
        STRUCT!{#[cfg_attr(feature = "impl-debug", derive(Debug))] $($rest)*}
    );
    ($(#[$attrs:meta])* struct $name:ident {
        $($field:ident: $ftype:ty,)+
    }) => (
        #[repr(C)] #[derive(Copy)] $(#[$attrs])*
        pub struct $name {
            $(pub $field: $ftype,)+
        }
        impl Clone for $name {
            #[inline]
            fn clone(&self) -> $name { *self }
        }
        #[cfg(feature = "impl-default")]
        impl Default for $name {
            #[inline]
            fn default() -> $name { unsafe { $crate::_core::mem::zeroed() } }
        }
    );
}

macro_rules! UNION {
    ($(#[$attrs:meta])* union $name:ident {
        $($variant:ident: $ftype:ty,)+
    }) => (
        #[repr(C)] $(#[$attrs])*
        pub union $name {
            $(pub $variant: $ftype,)+
        }
        impl Copy for $name {}
        impl Clone for $name {
            #[inline]
            fn clone(&self) -> $name { *self }
        }
        #[cfg(feature = "impl-default")]
        impl Default for $name {
            #[inline]
            fn default() -> $name { unsafe { $crate::_core::mem::zeroed() } }
        }
    );
}

EXTERN! {extern "system" {
    fn NtCancelIoFileEx(
        FileHandle: HANDLE,
        IoRequestToCancel: PIO_STATUS_BLOCK,
        IoStatusBlock: PIO_STATUS_BLOCK,
    ) -> NTSTATUS;
    fn NtDeviceIoControlFile(
        FileHandle: HANDLE,
        Event: HANDLE,
        ApcRoutine: PIO_APC_ROUTINE,
        ApcContext: PVOID,
        IoStatusBlock: PIO_STATUS_BLOCK,
        IoControlCode: ULONG,
        InputBuffer: PVOID,
        InputBufferLength: ULONG,
        OutputBuffer: PVOID,
        OutputBufferLength: ULONG,
    ) -> NTSTATUS;
    fn RtlNtStatusToDosError(
        Status: NTSTATUS,
    ) -> ULONG;
}}

FN! {stdcall PIO_APC_ROUTINE(
    ApcContext: PVOID,
    IoStatusBlock: PIO_STATUS_BLOCK,
    Reserved: ULONG,
) -> ()}

STRUCT! {struct IO_STATUS_BLOCK {
    u: IO_STATUS_BLOCK_u,
    Information: ULONG_PTR,
}}

UNION! {union IO_STATUS_BLOCK_u {
    Status: NTSTATUS,
    Pointer: PVOID,
}}

cfg_net!(
    use winapi::{
        shared::ntdef::{PHANDLE, PLARGE_INTEGER, POBJECT_ATTRIBUTES},
        um::winnt::ACCESS_MASK,
    };
    pub(crate) const FILE_OPEN: ULONG = 0x00000001;
    EXTERN! {extern "system" {
        fn NtCreateFile(
            FileHandle: PHANDLE,
            DesiredAccess: ACCESS_MASK,
            ObjectAttributes: POBJECT_ATTRIBUTES,
            IoStatusBlock: PIO_STATUS_BLOCK,
            AllocationSize: PLARGE_INTEGER,
            FileAttributes: ULONG,
            ShareAccess: ULONG,
            CreateDisposition: ULONG,
            CreateOptions: ULONG,
            EaBuffer: PVOID,
            EaLength: ULONG,
        ) -> NTSTATUS;
    }}
);
