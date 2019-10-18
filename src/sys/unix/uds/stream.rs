use super::{pair_descriptors, socket_addr, SocketAddr};
use crate::event::Source;
use crate::sys::unix::net::new_socket;
use crate::sys::unix::SourceFd;
use crate::{Interests, Registry, Token};

use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net;
use std::path::Path;

#[derive(Debug)]
pub struct UnixStream {
    inner: net::UnixStream,
}

impl UnixStream {
    pub fn new(inner: net::UnixStream) -> UnixStream {
        UnixStream { inner }
    }

    pub(crate) fn connect(path: &Path) -> io::Result<UnixStream> {
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

    pub(crate) fn pair() -> io::Result<(UnixStream, UnixStream)> {
        let fds = [-1; 2];
        let flags = libc::SOCK_STREAM;

        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "solaris")))]
        let pair = {
            pair_descriptors(fds, flags)?;
            unsafe {
                (
                    UnixStream::from_raw_fd(fds[0]),
                    UnixStream::from_raw_fd(fds[1]),
                )
            }
        };

        // Darwin and Solaris do not have SOCK_NONBLOCK or SOCK_CLOEXEC.
        //
        // In order to set those flags, additional `fcntl` sys calls must be
        // made in `pair_descriptors` that are fallible. If a `fnctl` fails
        // after the sockets have been created, the file descriptors will
        // leak. Creating `s1` and `s2` below ensure that if there is an
        // error, the file descriptors are closed.
        #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
        let pair = {
            let s1 = unsafe { UnixStream::from_raw_fd(fds[0]) };
            let s2 = unsafe { UnixStream::from_raw_fd(fds[1]) };
            pair_descriptors(fds, flags)?;
            (s1, s2)
        };

        Ok(pair)
    }

    pub(crate) fn try_clone(&self) -> io::Result<UnixStream> {
        let inner = self.inner.try_clone()?;
        Ok(UnixStream::new(inner))
    }

    pub(crate) fn local_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|sockaddr, socklen| {
            syscall!(getsockname(self.inner.as_raw_fd(), sockaddr, socklen))
        })
    }

    pub(crate) fn peer_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|sockaddr, socklen| {
            syscall!(getpeername(self.inner.as_raw_fd(), sockaddr, socklen))
        })
    }

    pub(crate) fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }

    pub(crate) fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }
}

impl Source for UnixStream {
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

impl<'a> Read for &'a UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.inner).read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        (&self.inner).read_vectored(bufs)
    }
}

impl<'a> Write for &'a UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.inner).write(buf)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        (&self.inner).write_vectored(bufs)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.inner).flush()
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl FromRawFd for UnixStream {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixStream {
        UnixStream::new(net::UnixStream::from_raw_fd(fd))
    }
}

impl IntoRawFd for UnixStream {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}
