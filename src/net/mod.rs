//! Networking primitives
//!
//! The types provided in this module are non-blocking by default and are
//! designed to be portable across all supported Mio platforms. As long as the
//! [portability guidelines] are followed, the behavior should be identical no
//! matter the target platform.
//!
//! [portability guidelines]: ../struct.Poll.html#portability

mod tcp;
mod udp;

pub use self::tcp::{TcpListener, TcpStream};
pub use self::udp::UdpSocket;

#[test]
#[cfg(not(debug_assertions))]
fn assert_size() {
    use std::mem::size_of;

    use crate::sys;

    // Without debug assertions enabled `TcpListener`, `TcpStream` and `UdpSocket` should have the
    // same size as the system specific socket, i.e. just a file descriptor on Unix platforms.
    assert_eq!(size_of::<TcpListener>(), size_of::<sys::TcpListener>());
    assert_eq!(size_of::<TcpStream>(), size_of::<sys::TcpStream>());
    assert_eq!(size_of::<UdpSocket>(), size_of::<sys::UdpSocket>());
}
