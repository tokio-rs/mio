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
    Shutdown,
};

#[cfg(unix)]
mod unix;
