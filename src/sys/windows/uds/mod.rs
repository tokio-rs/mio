mod stdnet;
pub use self::stdnet::SocketAddr;

fn path_offset(addr: &WinSock::sockaddr_un) -> usize {
    // Work with an actual instance of the type since using a null pointer is UB
    let base = addr as *const _ as usize;
    let path = &addr.sun_path as *const _ as usize;
    path - base
}

cfg_os_poll! {
    use windows_sys::Win32::Networking::WinSock;
    use std::os::windows::io::RawSocket;
    use std::path::Path;
    use std::{io, mem};

    pub(crate) mod listener;
    pub(crate) mod stream;

    pub unsafe fn socket_addr(path: &Path) -> io::Result<(WinSock::sockaddr_un, c_int)> {
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
            *dst = *src as c_char;
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

    pub(crate) fn local_addr(socket: RawSocket) -> io::Result<SocketAddr> {
        SocketAddr::new(|sockaddr, socklen| {
            wsa_syscall!(
                WinSock::getsockname(socket, sockaddr, socklen),
                PartialEq::eq,
                SOCKET_ERROR
            )
        })
    }

    pub(crate) fn peer_addr(socket: RawSocket) -> io::Result<SocketAddr> {
        SocketAddr::new(|sockaddr, socklen| {
            wsa_syscall!(
                WinSock::getpeername(socket, sockaddr, socklen),
                PartialEq::eq,
                SOCKET_ERROR
            )
        })
    }
}
