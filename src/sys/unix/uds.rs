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

/// An address associated with a mio specific unix socket.
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
    let (raw_addr, raw_addr_length) = socket_addr(path)?;
    let raw_addr = &raw_addr as *const _ as *const _;

    match syscall!(connect(socket, raw_addr, raw_addr_length)) {
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
    let (raw_addr, raw_addr_length) = socket_addr(path)?;
    let raw_addr = &raw_addr as *const _ as *const _;

    syscall!(bind(socket, raw_addr, raw_addr_length))?;
    Ok(unsafe { UnixDatagram::from_raw_fd(socket) })
}

// pub fn accept(listener: &UnixListener) -> io::Result<Option<(UnixStream, SocketAddr)>> {
pub fn accept(listener: &UnixListener) -> io::Result<(UnixStream, SocketAddr)> {
    let mut storage: libc::sockaddr_un = unsafe { mem::zeroed() };
    storage.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let mut len = mem::size_of_val(&storage) as libc::socklen_t;
    let raw_storage = &mut storage as *mut _ as *mut _;

    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    let sock_addr = {
        let flags = libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

        match syscall!(accept4(listener.as_raw_fd(), raw_storage, &mut len, flags)) {
            Ok(sa) => sa,
            Err(e) => return Err(e),
        }
    };

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    let sock_addr = match syscall!(accept(listener.as_raw_fd(), raw_storage, &mut len)) {
        Ok(sa) => sa,
        Err(e) => return Err(e),
    };

    #[cfg(any(target_os = "ios", target_os = "macos"))]
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
    let (raw_addr, raw_addr_length) = socket_addr(path)?;
    let raw_addr = &raw_addr as *const _ as *const _;

    syscall!(bind(socket, raw_addr, raw_addr_length))
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
    unsafe {
        let mut addr: libc::sockaddr_un = mem::zeroed();
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;

        let bytes = path.as_os_str().as_bytes();

        match (bytes.get(0), bytes.len().cmp(&addr.sun_path.len())) {
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

        for (dst, src) in addr.sun_path.iter_mut().zip(bytes.iter()) {
            *dst = *src as libc::c_char;
        }

        // null byte for pathname addresses is already there because we zeroed the
        // struct
        let mut len = sun_path_offset() + bytes.len();

        match bytes.get(0) {
            Some(&0) | None => {}
            Some(_) => len += 1,
        }

        Ok((addr, len as libc::socklen_t))
    }
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

fn sun_path_offset() -> usize {
    unsafe {
        // Work with an actual instance of the type since using a null pointer is UB
        let addr: libc::sockaddr_un = mem::uninitialized();
        let base = &addr as *const _ as usize;
        let path = &addr.sun_path as *const _ as usize;
        path - base
    }
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
        let path = unsafe { mem::transmute::<&[libc::c_char], &[u8]>(&self.addr.sun_path) };

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
