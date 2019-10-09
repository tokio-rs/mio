use std::cmp::Ordering;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::RawFd;
use std::path::Path;
use std::{io, mem};

mod datagram;
pub use self::datagram::UnixDatagram;

mod listener;
pub use self::listener::{SocketAddr, UnixListener};

mod stream;
pub use self::stream::UnixStream;

pub fn socket_addr(path: &Path) -> io::Result<(libc::sockaddr_un, libc::socklen_t)> {
    let sockaddr = mem::MaybeUninit::<libc::sockaddr_un>::zeroed();

    // This is safe to assume because a `libc::sockaddr_un` filled with `0`
    // bytes is properly initialized.
    //
    // `0` is a valid value for `sockaddr_un::sun_family`; it is
    // `libc::AF_UNSPEC`.
    //
    // `[0; 108]` is a valid value for `sockaddr_un::sun_path`; it begins an
    // abstract path.
    let mut sockaddr = unsafe { sockaddr.assume_init() };

    sockaddr.sun_family = libc::AF_UNIX as libc::sa_family_t;

    let bytes = path.as_os_str().as_bytes();
    match (bytes.get(0), bytes.len().cmp(&sockaddr.sun_path.len())) {
        // Abstract paths don't need a null terminator
        (Some(&0), Ordering::Greater) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path must be no longer than libc::sockaddr_un.sun_path",
            ));
        }
        (_, Ordering::Greater) | (_, Ordering::Equal) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path must be shorter than libc::sockaddr_un.sun_path",
            ));
        }
        _ => {}
    }

    for (dst, src) in sockaddr.sun_path.iter_mut().zip(bytes.iter()) {
        *dst = *src as libc::c_char;
    }

    // View `SocketAddr::path_offset` documentation for why this is necessary.
    let offset = {
        let base = &sockaddr as *const _ as usize;
        let path = &sockaddr as *const _ as usize;
        path - base
    };
    let mut socklen = offset + bytes.len();

    match bytes.get(0) {
        // The struct has already been zeroes so the null byte for pathname
        // addresses is already there.
        Some(&0) | None => {}
        Some(_) => socklen += 1,
    }

    Ok((sockaddr, socklen as libc::socklen_t))
}

fn pair_descriptors(mut fds: [RawFd; 2], flags: i32) -> io::Result<()> {
    #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "solaris")))]
    let flags = flags | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

    syscall!(socketpair(libc::AF_UNIX, flags, 0, fds.as_mut_ptr()))?;

    // Darwin and Solaris don't have SOCK_NONBLOCK or SOCK_CLOEXEC.
    //
    // For platforms that don't support flags in `socket`, the flags must be
    // set through `fcntl`. The `F_SETFL` command sets the `O_NONBLOCK` bit.
    // The `F_SETFD` command sets the `FD_CLOEXEC` bit.
    #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
    {
        syscall!(fcntl(fds[0], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC))?;
        syscall!(fcntl(fds[1], libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC))?;
    }
    Ok(())
}
