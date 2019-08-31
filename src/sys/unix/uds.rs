use crate::sys::unix::net::new_socket;

use std::cmp::Ordering;
use std::io;
use std::mem;
use std::os::unix::prelude::*;
use std::os::unix::net::{SocketAddr, UnixDatagram, UnixListener,UnixStream};
use std::path::Path;

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

pub fn accept(listener: &UnixListener) -> io::Result<(UnixStream, SocketAddr)> {
    unimplemented!();
}

pub fn socket_addr(path: &Path) -> io::Result<(libc::sockaddr_un, libc::socklen_t)> {
    unsafe {
        let mut addr: libc::sockaddr_un = mem::zeroed();
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;

        let bytes = path.as_os_str().as_bytes();

        match (bytes.get(0), bytes.len().cmp(&addr.sun_path.len())) {
            // Abstract paths don't need a null terminator
            (Some(&0), Ordering::Greater) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          "path must be no longer than SUN_LEN"));
            }
            (_, Ordering::Greater) | (_, Ordering::Equal) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          "path must be shorter than SUN_LEN"));
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

fn sun_path_offset() -> usize {
    unsafe {
        // Work with an actual instance of the type since using a null pointer is UB
        let addr: libc::sockaddr_un = mem::uninitialized();
        let base = &addr as *const _ as usize;
        let path = &addr.sun_path as *const _ as usize;
        path - base
    }
}
