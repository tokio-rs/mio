#[cfg(unix)]
pub use self::unix::{
    pipe, set_nonblock, Event, EventedFd, Events, Io, RawEvent, Selector, TcpListener, TcpStream,
    UdpSocket, Waker,
};

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{
    Binding, Event, Events, Overlapped, RawEvent, Selector, TcpListener, TcpStream, UdpSocket,
    Waker,
};

#[cfg(windows)]
mod windows;
