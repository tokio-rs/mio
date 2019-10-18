use super::{path_offset, socket_addr};
use crate::event::Source;
use crate::sys::unix::net::new_socket;
use crate::sys::unix::UnixStream;
use crate::unix::SourceFd;
use crate::{Interests, Registry, Token};

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net;
use std::path::Path;
use std::{ascii, fmt, io, mem};

#[derive(Debug)]
pub struct UnixListener {
    inner: net::UnixListener,
}

/// An address associated with a `mio` specific Unix socket.
///
/// This is implemented instead of imported from [`net::SocketAddr`] because
/// there is no way to create a [`net::SocketAddr`]. One must be returned by
/// [`accept`], so this is returned instead.
///
/// [`net::SocketAddr`]: std::os::unix::net::SocketAddr
/// [`accept`]: #method.accept
pub struct SocketAddr {
    sockaddr: libc::sockaddr_un,
    socklen: libc::socklen_t,
}

enum AddressKind<'a> {
    Unnamed,
    Pathname(&'a Path),
    Abstract(&'a [u8]),
}

impl UnixListener {
    fn new(inner: net::UnixListener) -> UnixListener {
        UnixListener { inner }
    }

    pub(crate) fn accept(&self) -> io::Result<(UnixStream, SocketAddr)> {
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
        let mut socklen = mem::size_of_val(&sockaddr) as libc::socklen_t;

        #[cfg(not(any(
            target_os = "ios",
            target_os = "macos",
            target_os = "netbsd",
            target_os = "solaris"
        )))]
        let socket = {
            let flags = libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;
            syscall!(accept4(
                self.inner.as_raw_fd(),
                &mut sockaddr as *mut libc::sockaddr_un as *mut libc::sockaddr,
                &mut socklen,
                flags
            ))
            .map(|socket| unsafe { UnixStream::from_raw_fd(socket) })
        };

        #[cfg(any(
            target_os = "ios",
            target_os = "macos",
            target_os = "netbsd",
            target_os = "solaris"
        ))]
        let socket = syscall!(accept(
            self.inner.as_raw_fd(),
            &mut sockaddr as *mut libc::sockaddr_un as *mut libc::sockaddr,
            &mut socklen,
        ))
        .and_then(|socket| {
            // Ensure the socket is closed if either of the `fcntl` calls
            // error below.
            let s = unsafe { UnixStream::from_raw_fd(socket) };
            syscall!(fcntl(socket, libc::F_SETFL, libc::O_NONBLOCK))
                .and_then(|_| syscall!(fcntl(socket, libc::F_SETFD, libc::FD_CLOEXEC)).map(|_| s))
        });

        socket.and_then(|s| Ok((s, SocketAddr::from_parts(sockaddr, socklen))))
    }

    pub(crate) fn bind(path: &Path) -> io::Result<UnixListener> {
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

    pub(crate) fn try_clone(&self) -> io::Result<UnixListener> {
        let inner = self.inner.try_clone()?;
        Ok(UnixListener::new(inner))
    }

    pub(crate) fn local_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|sockaddr, socklen| {
            syscall!(getsockname(self.inner.as_raw_fd(), sockaddr, socklen))
        })
    }

    /// Returns the value of the `SO_ERROR` option.
    pub(crate) fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }
}

impl Source for UnixListener {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).deregister(registry)
    }
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl IntoRawFd for UnixListener {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl FromRawFd for UnixListener {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixListener {
        UnixListener::new(net::UnixListener::from_raw_fd(fd))
    }
}

impl SocketAddr {
    pub(crate) fn new<F>(f: F) -> io::Result<SocketAddr>
    where
        F: FnOnce(*mut libc::sockaddr, &mut libc::socklen_t) -> io::Result<libc::c_int>,
    {
        let mut sockaddr = {
            let sockaddr = mem::MaybeUninit::<libc::sockaddr_un>::zeroed();
            unsafe { sockaddr.assume_init() }
        };

        let raw_sockaddr = &mut sockaddr as *mut libc::sockaddr_un as *mut libc::sockaddr;
        let mut socklen = mem::size_of_val(&sockaddr) as libc::socklen_t;

        f(raw_sockaddr, &mut socklen)?;
        Ok(SocketAddr::from_parts(sockaddr, socklen))
    }

    pub(crate) fn from_parts(sockaddr: libc::sockaddr_un, socklen: libc::socklen_t) -> SocketAddr {
        SocketAddr { sockaddr, socklen }
    }

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
        let offset = path_offset(&self.sockaddr);
        let len = self.socklen as usize - offset;
        let path = unsafe { &*(&self.sockaddr.sun_path as *const [libc::c_char] as *const [u8]) };

        // macOS seems to return a len of 16 and a zeroed sun_path for unnamed addresses
        if len == 0
            || (cfg!(not(any(target_os = "linux", target_os = "android")))
                && self.sockaddr.sun_path[0] == 0)
        {
            AddressKind::Unnamed
        } else if self.sockaddr.sun_path[0] == 0 {
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
