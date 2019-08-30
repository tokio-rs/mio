use std::io;
use std::os::unix::net::{UnixDatagram, UnixListener,UnixStream};
use std::path::Path;

pub fn connect_stream(_path: &Path) -> io::Result<UnixStream> {
    unimplemented!();
    /*
    unsafe {
        // On linux we first attempt to pass the SOCK_CLOEXEC flag to
        // atomically create the socket and set it as CLOEXEC. Support for
        // this option, however, was added in 2.6.27, and we still support
        // 2.6.18 as a kernel, so if the returned error is EINVAL we
        // fallthrough to the fallback.
        if cfg!(target_os = "linux") || cfg!(target_os = "android") {
            let flags = ty | SOCK_CLOEXEC | SOCK_NONBLOCK;
            match cvt(libc::socket(libc::AF_UNIX, flags, 0)) {
                Ok(fd) => return Ok(Socket { fd: fd }),
                Err(ref e) if e.raw_os_error() == Some(libc::EINVAL) => {}
                Err(e) => return Err(e),
            }
        }

        let fd = Socket { fd: try!(cvt(libc::socket(libc::AF_UNIX, ty, 0))) };
        try!(cvt(libc::ioctl(fd.fd, libc::FIOCLEX)));
        let mut nonblocking = 1 as c_ulong;
        try!(cvt(libc::ioctl(fd.fd, libc::FIONBIO, &mut nonblocking)));
        Ok(fd)
    }
    */
}

pub fn pair_stream() -> io::Result<(UnixStream, UnixStream)> {
    unimplemented!()
}

pub fn bind_datagram(_path: &Path) -> io::Result<UnixDatagram> {
    unimplemented!();
}

pub fn bind_listener(_path: &Path) -> io::Result<UnixListener> {
    unimplemented!();
}

pub fn pair_datagram() -> io::Result<(UnixDatagram, UnixDatagram)> {
    unimplemented!();
}

pub fn unbound_datagram() -> io::Result<UnixDatagram> {
    unimplemented!();
}
