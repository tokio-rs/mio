#[cfg(unix)]
mod datagram;
#[cfg(unix)]
pub use self::datagram::UnixDatagram;

mod listener;
pub use self::listener::UnixListener;

mod stream;
pub use self::stream::UnixStream;

// This is a stand-in until std::os::windows::net::SocketAddr is stabilized.
#[cfg(windows)]
pub use crate::sys::uds::SocketAddr;
