use std::io::{Read, Write};
use std::net::{self, SocketAddr};
use std::os::unix::io::{RawFd, FromRawFd, IntoRawFd, AsRawFd};
use std::time::Duration;

use iovec::IoVec;

use {io, Evented, Ready, Poll, PollOpt, Token};

use sys::redox::eventedfd::EventedFd;
use sys::redox::io::set_nonblock;

use syscall;

#[derive(Debug)]
pub struct TcpStream {
    inner: net::TcpStream,
}

#[derive(Debug)]
pub struct TcpListener {
    inner: net::TcpListener,
}

impl TcpStream {
    pub fn connect(stream: net::TcpStream, addr: &SocketAddr) -> io::Result<TcpStream> {
        let fd = stream.as_raw_fd();
        set_nonblock(fd)?;

        let path = match *addr {
            SocketAddr::V4(addrv4) => {
                let ip = addrv4.ip().octets();
                let port = addrv4.port();
                format!("{}.{}.{}.{}:{}", ip[0], ip[1], ip[2], ip[3], port)
            },
            SocketAddr::V6(_addrv6) => {
                return Err(io::Error::new(io::ErrorKind::Other, "Not implemented"));
            }
        };

        let new_fd = syscall::dup(fd, path.as_bytes()).map_err(|err| {
            io::Error::from_raw_os_error(err.errno)
        })?;
        let ret = syscall::dup2(new_fd, fd, &[]).map_err(|err| {
            io::Error::from_raw_os_error(err.errno)
        });
        let _ = syscall::close(new_fd);

        ret?;

        Ok(TcpStream {
            inner: stream,
        })
    }

    pub fn from_stream(stream: net::TcpStream) -> TcpStream {
        TcpStream {
            inner: stream,
        }
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.inner.try_clone().map(|s| {
            TcpStream {
                inner: s,
            }
        })
    }

    pub fn shutdown(&self, how: net::Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.inner.set_nodelay(nodelay)
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        self.inner.nodelay()
    }

    pub fn set_recv_buffer_size(&self, _size: usize) -> io::Result<()> {
        //self.inner.set_recv_buffer_size(size)
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn recv_buffer_size(&self) -> io::Result<usize> {
        //self.inner.recv_buffer_size()
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn set_send_buffer_size(&self, _size: usize) -> io::Result<()> {
        //self.inner.set_send_buffer_size(size)
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn send_buffer_size(&self) -> io::Result<usize> {
        //self.inner.send_buffer_size()
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn set_keepalive(&self, _keepalive: Option<Duration>) -> io::Result<()> {
        //self.inner.set_keepalive(keepalive)
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn keepalive(&self) -> io::Result<Option<Duration>> {
        //self.inner.keepalive()
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.inner.set_ttl(ttl)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        self.inner.ttl()
    }

    pub fn set_only_v6(&self, _only_v6: bool) -> io::Result<()> {
        //self.inner.set_only_v6(only_v6)
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn only_v6(&self) -> io::Result<bool> {
        //self.inner.only_v6()
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn set_linger(&self, _dur: Option<Duration>) -> io::Result<()> {
        //self.inner.set_linger(dur)
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn linger(&self) -> io::Result<Option<Duration>> {
        //self.inner.linger()
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }

    pub fn readv(&self, _bufs: &mut [&mut IoVec]) -> io::Result<usize> {
        unimplemented!("readv");
    }

    pub fn writev(&self, _bufs: &[&IoVec]) -> io::Result<usize> {
        unimplemented!("writev");
    }
}

impl<'a> Read for &'a TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.inner).read(buf)
    }
}

impl<'a> Write for &'a TcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.inner).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.inner).flush()
    }
}

impl Evented for TcpStream {
    fn register(&self, poll: &Poll, token: Token,
                interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token,
                  interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}

impl FromRawFd for TcpStream {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpStream {
        TcpStream {
            inner: net::TcpStream::from_raw_fd(fd),
        }
    }
}

impl IntoRawFd for TcpStream {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl TcpListener {
    pub fn new(inner: net::TcpListener, _addr: &SocketAddr) -> io::Result<TcpListener> {
        set_nonblock(inner.as_raw_fd())?;
        Ok(TcpListener {
            inner: inner,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn try_clone(&self) -> io::Result<TcpListener> {
        self.inner.try_clone().map(|s| {
            TcpListener {
                inner: s,
            }
        })
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        self.inner.accept().and_then(|(s, a)| {
            set_nonblock(s.as_raw_fd())?;
            Ok((TcpStream {
                inner: s,
            }, a))
        })
    }

    #[allow(deprecated)]
    pub fn set_only_v6(&self, only_v6: bool) -> io::Result<()> {
        self.inner.set_only_v6(only_v6)
    }

    #[allow(deprecated)]
    pub fn only_v6(&self) -> io::Result<bool> {
        self.inner.only_v6()
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.inner.set_ttl(ttl)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        self.inner.ttl()
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }
}

impl Evented for TcpListener {
    fn register(&self, poll: &Poll, token: Token,
                interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token,
                  interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> TcpListener {
        TcpListener {
            inner: net::TcpListener::from_raw_fd(fd),
        }
    }
}

impl IntoRawFd for TcpListener {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
