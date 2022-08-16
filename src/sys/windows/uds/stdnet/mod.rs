//! Windows specific networking functionality

use std::ascii;
use std::fmt;
use std::io;
use std::mem;
use std::os::raw::c_int;
use std::path::Path;

use windows_sys::Win32::Networking::WinSock::{self, SOCKADDR};

mod net;
mod socket;

pub(crate) fn path_offset(addr: &WinSock::sockaddr_un) -> usize {
    // Work with an actual instance of the type since using a null pointer is UB
    let base = addr as *const _ as usize;
    let path = &addr.sun_path as *const _ as usize;
    path - base
}

pub(crate) fn socket_addr(path: &Path) -> io::Result<(WinSock::sockaddr_un, c_int)> {
    let sockaddr = mem::MaybeUninit::<WinSock::sockaddr_un>::zeroed();

    // This is safe to assume because a `WinSock::sockaddr_un` filled with `0`
    // bytes is properly initialized.
    //
    // `0` is a valid value for `sockaddr_un::sun_family`; it is
    // `WinSock::AF_UNSPEC`.
    //
    // `[0; 108]` is a valid value for `sockaddr_un::sun_path`; it begins an
    // abstract path.
    let mut sockaddr = unsafe { sockaddr.assume_init() };
    sockaddr.sun_family = WinSock::AF_UNIX;

    // Winsock2 expects 'sun_path' to be a Win32 UTF-8 file system path
    let bytes = path.to_str().map(|s| s.as_bytes()).ok_or(io::Error::new(
        io::ErrorKind::InvalidInput,
        "path contains invalid characters",
    ))?;

    if bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "paths may not contain interior null bytes",
        ));
    }

    if bytes.len() >= sockaddr.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be shorter than SUN_LEN",
        ));
    }
    for (dst, src) in sockaddr.sun_path.iter_mut().zip(bytes.iter()) {
        *dst = *src as u8;
    }
    // null byte for pathname addresses is already there because we zeroed the
    // struct

    let offset = path_offset(&sockaddr);
    let mut socklen = offset + bytes.len();

    match bytes.get(0) {
        // The struct has already been zeroes so the null byte for pathname
        // addresses is already there.
        Some(&0) | None => {}
        Some(_) => socklen += 1,
    }

    Ok((sockaddr, socklen as c_int))
}

enum AddressKind<'a> {
    Unnamed,
    Pathname(&'a Path),
    Abstract(&'a [u8]),
}

/// An address associated with a Unix socket
#[derive(Copy, Clone)]
pub struct SocketAddr {
    addr: WinSock::sockaddr_un,
    len: c_int,
}

impl SocketAddr {
    pub(crate) fn new<F>(f: F) -> io::Result<SocketAddr>
    where
        F: FnOnce(*mut SOCKADDR, *mut c_int) -> io::Result<c_int>,
    {
        let mut sockaddr = {
            let sockaddr = mem::MaybeUninit::<WinSock::sockaddr_un>::zeroed();
            unsafe { sockaddr.assume_init() }
        };

        let mut len = mem::size_of::<WinSock::sockaddr_un>() as c_int;
        f(&mut sockaddr as *mut _ as *mut _, &mut len)?;
        Ok(SocketAddr::from_parts(sockaddr, len))
    }

    pub(crate) fn from_parts(addr: WinSock::sockaddr_un, mut len: c_int) -> SocketAddr {
        if len == 0 {
            // When there is a datagram from unnamed unix socket
            // linux returns zero bytes of address
            len = path_offset(&addr) as c_int; // i.e. zero-length address
        }
        SocketAddr { addr, len }
    }

    /// Returns true if and only if the address is unnamed.
    pub fn is_unnamed(&self) -> bool {
        if let AddressKind::Unnamed = self.address() {
            true
        } else {
            false
        }
    }

    /// Returns the contents of this address if it is a `pathname` address.
    pub fn as_pathname(&self) -> Option<&Path> {
        if let AddressKind::Pathname(path) = self.address() {
            Some(path)
        } else {
            None
        }
    }

    fn address<'a>(&'a self) -> AddressKind<'a> {
        let len = self.len as usize - path_offset(&self.addr);
        // WinSock::sockaddr_un::sun_path on Windows is a Win32 UTF-8 file system path

        // macOS seems to return a len of 16 and a zeroed sun_path for unnamed addresses
        if len == 0
            || (cfg!(not(any(target_os = "linux", target_os = "android")))
                && self.addr.sun_path[0] == 0)
        {
            AddressKind::Unnamed
        } else if self.addr.sun_path[0] == 0 {
            AddressKind::Abstract(&self.addr.sun_path[1..len])
        } else {
            use std::ffi::CStr;
            let pathname = unsafe { CStr::from_bytes_with_nul_unchecked(&self.addr.sun_path[..len]) };
            AddressKind::Pathname(Path::new(pathname.to_str().unwrap()))
        }
    }
}

impl fmt::Debug for SocketAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.address() {
            AddressKind::Unnamed => write!(fmt, "(unnamed)"),
            AddressKind::Abstract(name) => write!(fmt, "{} (abstract)", AsciiEscaped(name)),
            AddressKind::Pathname(path) => write!(fmt, "{:?} (pathname)", path),
        }
    }
}

impl PartialEq for SocketAddr {
    fn eq(&self, other: &SocketAddr) -> bool {
        let ita = self.addr.sun_path.iter();
        let itb = other.addr.sun_path.iter();

        self.len == other.len
            && self.addr.sun_family == other.addr.sun_family
            && ita.zip(itb).all(|(a, b)| a == b)
    }
}

struct AsciiEscaped<'a>(&'a [u8]);

impl<'a> fmt::Display for AsciiEscaped<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "\"")?;
        for byte in self.0.iter().cloned().flat_map(ascii::escape_default) {
            write!(fmt, "{}", byte as char)?;
        }
        write!(fmt, "\"")
    }
}

pub use self::net::{UnixListener, UnixStream};
