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
    pipe,
    set_nonblock,
    IoVec,
};

#[cfg(unix)]
#[cfg(feature = "with-deprecated")]
pub use self::unix::UnixSocket;

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
    Overlapped,
    Binding,
    IoVec,
};

#[cfg(windows)]
mod windows;
