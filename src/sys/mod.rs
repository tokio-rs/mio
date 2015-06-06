#[cfg(unix)]
pub use self::unix::{
    Awakener,
    Events,
    EventsIterator,
    Io,
    Selector,
    TcpSocket,
    UdpSocket,
    UnixSocket,
    pipe,
};

#[cfg(unix)]
mod unix;
