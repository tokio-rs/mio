#[cfg(unix)]
pub use self::unix::{
    Awakener,
    EventedFd,
    Events,
    Io,
    Selector,
    TcpStream,
    TcpListener,
    UdpSocket,
    UnixSocket,
    pipe,
    set_nonblock,
};


#[cfg(unix)]
mod unix;

#[cfg(windows)]
pub use self::windows::{
    Awakener,
    Events,
    Selector,
    TcpStream,
    TcpListener,
    UdpSocket,
};

#[cfg(windows)]
mod windows;
