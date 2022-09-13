use std::{fmt, io, mem};
use std::os::raw::c_int;
use std::path::Path;
use crate::net::AddressKind;

use windows_sys::Win32::Networking::WinSock::{sockaddr_un, SOCKADDR};

fn path_offset(addr: &sockaddr_un) -> usize {
    // Work with an actual instance of the type since using a null pointer is UB
    let base = addr as *const _ as usize;
    let path = &addr.sun_path as *const _ as usize;
    path - base
}

cfg_os_poll! {
    use windows_sys::Win32::Networking::WinSock::AF_UNIX;
    pub(super) fn socket_addr(path: &Path) -> io::Result<(sockaddr_un, c_int)> {
        let sockaddr = mem::MaybeUninit::<sockaddr_un>::zeroed();

        // This is safe to assume because a `sockaddr_un` filled with `0`
        // bytes is properly initialized.
        //
        // `0` is a valid value for `sockaddr_un::sun_family`; it is
        // `WinSock::AF_UNSPEC`.
        //
        // `[0; 108]` is a valid value for `sockaddr_un::sun_path`; it begins an
        // abstract path.
        let mut sockaddr = unsafe { sockaddr.assume_init() };
        sockaddr.sun_family = AF_UNIX;

        // Winsock2 expects 'sun_path' to be a Win32 UTF-8 file system path
        let bytes = path.to_str().map(|s| s.as_bytes()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "path contains invalid characters",
            )
        })?;

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

        sockaddr.sun_path[..bytes.len()].copy_from_slice(bytes);

        let offset = path_offset(&sockaddr);
        let mut socklen = offset + bytes.len();

        match bytes.first() {
            // The struct has already been zeroes so the null byte for pathname
            // addresses is already there.
            Some(&0) | None => {}
            Some(_) => socklen += 1,
        }

        Ok((sockaddr, socklen as c_int))
    }
}

pub(crate) struct SocketAddr {
    addr: sockaddr_un,
    len: c_int,
}

impl SocketAddr {
    pub(crate) fn init<F, T>(f: F) -> io::Result<(T, SocketAddr)>
    where
        F: FnOnce(*mut SOCKADDR, *mut c_int) -> io::Result<T>,
    {
        let mut sockaddr = {
            let sockaddr = mem::MaybeUninit::<sockaddr_un>::zeroed();
            unsafe { sockaddr.assume_init() }
        };

        let mut len = mem::size_of::<sockaddr_un>() as c_int;
        let result = f(&mut sockaddr as *mut _ as *mut _, &mut len)?;
        Ok((
            result,
            SocketAddr {
                addr: sockaddr,
                len,
            },
        ))
    }

    pub(crate) fn new<F>(f: F) -> io::Result<SocketAddr>
    where
        F: FnOnce(*mut SOCKADDR, *mut c_int) -> io::Result<c_int>,
    {
        SocketAddr::init(f).map(|(_, addr)| addr)
    }

    pub(crate) fn address(&self) -> AddressKind<'_> {
        let len = self.len as usize - path_offset(&self.addr);
        // sockaddr_un::sun_path on Windows is a Win32 UTF-8 file system path

        // macOS seems to return a len of 16 and a zeroed sun_path for unnamed addresses
        if len == 0 {
            AddressKind::Unnamed
        } else if self.addr.sun_path[0] == 0 {
            AddressKind::Abstract(&self.addr.sun_path[1..len])
        } else {
            use std::ffi::CStr;
            let pathname =
                unsafe { CStr::from_bytes_with_nul_unchecked(&self.addr.sun_path[..len]) };
            AddressKind::Pathname(Path::new(pathname.to_str().unwrap()))
        }
    }
}

impl fmt::Debug for SocketAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{:?}", self.address())
    }
}
