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

#[cfg(windows)]
pub use self::windows::{
    Awakener,
    Events,
    Selector,
    TcpSocket,
    UdpSocket,
};

#[cfg(windows)]
mod windows;
