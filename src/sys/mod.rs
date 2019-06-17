#[cfg(unix)]
pub use self::unix::{
    pipe, set_nonblock, Event, EventedFd, Events, Io, Selector, SysEvent, TcpListener, TcpStream,
    UdpSocket, Waker,
};

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{
    Binding, Event, Events, Overlapped, Selector, SysEvent, TcpListener, TcpStream, UdpSocket,
    Waker,
};

#[cfg(windows)]
mod windows;
