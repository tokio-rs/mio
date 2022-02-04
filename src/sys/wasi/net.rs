#![allow(dead_code)]

use std::io;

/// A lot of function are not support on Wasi, this function returns a
/// consistent error when calling those functions.
fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Other, "not supported on wasi")
}

pub(crate) mod tcp {
    use std::io;
    use std::net;

    use super::unsupported;

    pub type TcpSocket = wasi::Fd;

    pub(crate) fn new_for_addr(_address: net::SocketAddr) -> io::Result<i32> {
        Err(unsupported())
    }

    pub(crate) fn bind(_: &net::TcpListener, _: net::SocketAddr) -> io::Result<()> {
        Ok(())
    }

    pub(crate) fn connect(_: &net::TcpStream, _: net::SocketAddr) -> io::Result<net::TcpStream> {
        Err(unsupported())
    }

    pub(crate) fn listen(_: &net::TcpListener, _: u32) -> io::Result<()> {
        Ok(())
    }

    pub(crate) fn set_reuseaddr(_: &net::TcpListener, _: bool) -> io::Result<()> {
        Ok(())
    }
    pub(crate) fn accept(
        listener: &net::TcpListener,
    ) -> io::Result<(net::TcpStream, net::SocketAddr)> {
        let res = listener.accept();
        res
    }
}

pub(crate) mod udp {
    use std::io;
    use std::net::{self, SocketAddr};
    use std::os::wasi::io::FromRawFd;

    //use super::unsupported;

    pub(crate) fn bind(_: SocketAddr) -> io::Result<net::UdpSocket> {
        Ok(unsafe { net::UdpSocket::from_raw_fd(0) })
    }

    pub(crate) fn only_v6(_socket: &net::UdpSocket) -> io::Result<bool> {
        Ok(false)
    }
}
