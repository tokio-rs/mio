//! Networking primitives
//!
//! The types provided in this module are non-blocking by default and are
//! designed to be portable across all supported Mio platforms. As long as the
//! [portability guidelines] are followed, the behavior should be identical no
//! matter the target platform.
//!
//! [portability guidelines]: ../struct.Poll.html#portability

#[cfg(feature = "tcp")]
mod tcp;
#[cfg(feature = "tcp")]
pub use self::tcp::{TcpListener, TcpStream};

#[cfg(feature = "udp")]
mod udp;
#[cfg(feature = "udp")]
pub use self::udp::UdpSocket;

#[cfg(all(unix, feature = "uds"))]
mod uds;
#[cfg(all(unix, feature = "uds"))]
pub use self::uds::{UnixDatagram, UnixListener, UnixStream};
