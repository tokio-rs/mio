//! Networking primitives
//!
//! The types provided in this module are non-blocking by default and are
//! designed to be portable across all supported Mio platforms. As long as the
//! [portability guidelines] are followed, the behavior should be identical no
//! matter the target platform.
//!
//! [portability guidelines]: ../struct.Poll.html#portability

mod tcp;

#[cfg(not(target_os="emscripten"))]
mod udp;

pub use self::tcp::{TcpListener, TcpStream};

#[cfg(not(target_os="emscripten"))]
pub use self::udp::UdpSocket;
