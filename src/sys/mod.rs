#[cfg(unix)]
pub use self::unix::{
    pipe, set_nonblock, EventedFd, Events, Io, Selector, TcpListener, TcpStream, UdpSocket, Waker,
};

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{
    Binding, Events, Overlapped, Selector, TcpListener, TcpStream, UdpSocket, Waker,
};

#[cfg(windows)]
mod windows;
