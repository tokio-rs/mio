#[cfg(unix)]
pub use self::unix::{
    pipe, set_nonblock, Awakener, EventedFd, Events, Io, Selector, TcpListener, TcpStream,
    UdpSocket,
};

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{
    Awakener, Binding, Events, Overlapped, Selector, TcpListener, TcpStream, UdpSocket,
};

#[cfg(windows)]
mod windows;
