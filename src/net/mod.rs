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

#[cfg(not(target_os="emscripten"))]
pub use self::tcp::TcpListener;

pub use self::tcp::TcpStream;

#[cfg(not(target_os="emscripten"))]
pub use self::udp::UdpSocket;
