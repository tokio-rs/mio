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
    IoVec,
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
    IoVec,
};

#[cfg(windows)]
mod windows;
