//! Networking primitives
//!
pub mod tcp;
pub mod udp;

#[cfg(unix)]
pub mod unix;

use std::io;
use std::net::{SocketAddr, ToSocketAddrs};

fn each_addr<A: ToSocketAddrs, F, T>(addr: A, mut f: F) -> io::Result<T>
    where F: FnMut(&SocketAddr) -> io::Result<T>
{
    let mut last_err = None;
    for addr in try!(addr.to_socket_addrs()) {
        match f(&addr) {
            Ok(l) => return Ok(l),
            Err(e) => last_err = Some(e),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput,
                   "could not resolve to any addresses")
    }))
}
