#[cfg(unix)]
mod datagram;
#[cfg(unix)]
#[cfg_attr(docsrs, doc(cfg(unix)))]
pub use self::datagram::UnixDatagram;

mod listener;
pub use self::listener::UnixListener;

mod stream;
pub use self::stream::UnixStream;

mod addr;
pub(crate) use self::addr::AddressKind;
pub use self::addr::SocketAddr;
