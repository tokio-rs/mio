#[cfg(unix)]
pub use self::unix::{
    Awakener,
    Events,
    Io,
    Selector,
    TcpSocket,
    UdpSocket,
    UnixSocket,
    pipe,
};

#[cfg(unix)]
mod unix;
