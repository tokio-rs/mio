use crate::sys::unix::net::new_socket;

use std::ascii;
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::mem;
use std::os::unix::net::{UnixDatagram, UnixListener, UnixStream};
use std::os::unix::prelude::*;
use std::path::Path;

/// An address associated with a `mio` specific Unix socket.
///
/// This is implemented instead of imported from [`net::SocketAddr`] because
/// there is no way to create a [`net::SocketAddr`]. One must be returned by
/// [`accept`], so this is returned instead.
///
/// [`net::SocketAddr`]: std::os::unix::net::SocketAddr
/// [`accept`]: #method.accept
pub struct SocketAddr {
    addr: libc::sockaddr_un,
    len: libc::socklen_t,
}

enum AddressKind<'a> {
    Unnamed,
    Pathname(&'a Path),
    Abstract(&'a [u8]),
}

pub fn connect_stream(path: &Path) -> io::Result<UnixStream> {
    let socket = new_socket(libc::AF_UNIX, libc::SOCK_STREAM)?;
    let (sockaddr, socklen) = socket_addr(path)?;
    let sockaddr = &sockaddr as *const libc::sockaddr_un as *const libc::sockaddr;

    match syscall!(connect(socket, sockaddr, socklen)) {
        Ok(_) => {}
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
        Err(e) => {
            // Close the socket if we hit an error, ignoring the error
            // from closing since we can't pass back two errors.
            let _ = unsafe { libc::close(socket) };

            return Err(e);
        }
    }

    Ok(unsafe { UnixStream::from_raw_fd(socket) })
}

pub fn pair_stream() -> io::Result<(UnixStream, UnixStream)> {
    let fds = [0, 0];
    let flags = libc::SOCK_STREAM;

    pair_descriptors(fds, flags)?;

    Ok(unsafe {
        (
            UnixStream::from_raw_fd(fds[0]),
            UnixStream::from_raw_fd(fds[1]),
        )
    })
}

pub fn bind_datagram(path: &Path) -> io::Result<UnixDatagram> {
    let socket = new_socket(libc::AF_UNIX, libc::SOCK_DGRAM)?;
    let (sockaddr, socklen) = socket_addr(path)?;
    let sockaddr = &sockaddr as *const libc::sockaddr_un as *const libc::sockaddr;

    syscall!(bind(socket, sockaddr, socklen))?;
    Ok(unsafe { UnixDatagram::from_raw_fd(socket) })
}

pub fn accept(listener: &UnixListener) -> io::Result<(UnixStream, SocketAddr)> {
    let mut storage: libc::sockaddr_un = unsafe { mem::zeroed() };
    storage.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let mut len = mem::size_of_val(&storage) as libc::socklen_t;
    let raw_storage = &mut storage as *mut libc::sockaddr_un as *mut libc::sockaddr;

    #[cfg(not(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "solaris"
    )))]
    let sock_addr = {
        let flags = libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;
        syscall!(accept4(listener.as_raw_fd(), raw_storage, &mut len, flags))?
    };

    #[cfg(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "solaris"
    ))]
    let sock_addr = syscall!(accept(listener.as_raw_fd(), raw_storage, &mut len))?;

    #[cfg(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "solaris"
    ))]
    {
        syscall!(fcntl(sock_addr, libc::F_SETFL, libc::O_NONBLOCK))?;
        syscall!(fcntl(sock_addr, libc::F_SETFD, libc::FD_CLOEXEC))?;
    }

    Ok((
        unsafe { UnixStream::from_raw_fd(sock_addr) },
        SocketAddr { addr: storage, len },
    ))
}

pub fn pair_datagram() -> io::Result<(UnixDatagram, UnixDatagram)> {
    let fds = [0, 0];
    let flags = libc::SOCK_DGRAM;

    pair_descriptors(fds, flags)?;

    Ok(unsafe {
        (
            UnixDatagram::from_raw_fd(fds[0]),
            UnixDatagram::from_raw_fd(fds[1]),
        )
    })
}

pub fn unbound_datagram() -> io::Result<UnixDatagram> {
    let socket = new_socket(libc::AF_UNIX, libc::SOCK_DGRAM)?;
    Ok(unsafe { UnixDatagram::from_raw_fd(socket) })
}

pub fn bind_listener(path: &Path) -> io::Result<UnixListener> {
    let socket = new_socket(libc::AF_UNIX, libc::SOCK_STREAM)?;
    let (sockaddr, socklen) = socket_addr(path)?;
    let sockaddr = &sockaddr as *const libc::sockaddr_un as *const libc::sockaddr;

    syscall!(bind(socket, sockaddr, socklen))
        .and_then(|_| syscall!(listen(socket, 1024)))
        .map_err(|err| {
            // Close the socket if we hit an error, ignoring the error from
            // closing since we can't pass back two errors.
            let _ = unsafe { libc::close(socket) };
            err
        })
        .map(|_| unsafe { UnixListener::from_raw_fd(socket) })
}

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
                "path must be no longer than SUN_LEN",
            ));
        }
        (_, Ordering::Greater) | (_, Ordering::Equal) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path must be shorter than SUN_LEN",
            ));
        }
        _ => {}
    }

    for (dst, src) in sockaddr.sun_path.iter_mut().zip(bytes.iter()) {
        *dst = *src as libc::c_char;
    }

    let mut socklen = sun_path_offset() + bytes.len();
    match bytes.get(0) {
        // The struct has already been zeroes so the null byte for pathname
        // addresses is already there.
        Some(&0) | None => {}
        Some(_) => socklen += 1,
    }

    Ok((sockaddr, socklen as libc::socklen_t))
}

fn pair_descriptors(mut fds: [i32; 2], flags: i32) -> io::Result<()> {
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

// On Linux, this funtion equates to the same value as
// `size_of::<sun_path>()`, but some other implementations include other
// fields before `sun_path`, so the expression more portably describes the
// size of the address structure.
fn sun_path_offset() -> usize {
    let sockaddr = unsafe { mem::MaybeUninit::<libc::sockaddr_un>::uninit().assume_init() };
    let base = &sockaddr as *const _ as usize;
    let path = &sockaddr.sun_path as *const _ as usize;
    path - base
}

impl SocketAddr {
    /// Returns `true` if the address is unnamed.
    ///
    /// Documentation reflected in [`SocketAddr`]
    ///
    /// [`SocketAddr`]: std::os::unix::net::SocketAddr
    pub fn is_unnamed(&self) -> bool {
        if let AddressKind::Unnamed = self.address() {
            true
        } else {
            false
        }
    }

    /// Returns the contents of this address if it is a `pathname` address.
    ///
    /// Documentation reflected in [`SocketAddr`]
    ///
    /// [`SocketAddr`]: std::os::unix::net::SocketAddr
    pub fn as_pathname(&self) -> Option<&Path> {
        if let AddressKind::Pathname(path) = self.address() {
            Some(path)
        } else {
            None
        }
    }

    fn address(&self) -> AddressKind<'_> {
        let len = self.len as usize - sun_path_offset();
        let path = unsafe { &*(&self.addr.sun_path as *const [libc::c_char] as *const [u8]) };

        // macOS seems to return a len of 16 and a zeroed sun_path for unnamed addresses
        if len == 0
            || (cfg!(not(any(target_os = "linux", target_os = "android")))
                && self.addr.sun_path[0] == 0)
        {
            AddressKind::Unnamed
        } else if self.addr.sun_path[0] == 0 {
            AddressKind::Abstract(&path[1..len])
        } else {
            AddressKind::Pathname(OsStr::from_bytes(&path[..len - 1]).as_ref())
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
