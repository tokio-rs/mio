//! Networking primitives
//!
//! The types provided in this module are non-blocking by default and are
//! designed to be portable across all supported Mio platforms. As long as the
//! [portability guidelines] are followed, the behavior should be identical no
//! matter the target platform.
//!
//! [portability guidelines]: ../struct.Poll.html#portability

mod tcp_listener;
pub use self::tcp_listener::TcpListener;

mod tcp_stream;
pub use self::tcp_stream::TcpStream;

mod udp;
pub use self::udp::UdpSocket;

#[cfg(unix)]
mod uds;
#[cfg(unix)]
pub use self::uds::{UnixDatagram, UnixListener, UnixStream};
