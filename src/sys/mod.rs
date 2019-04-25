#[cfg(unix)]
pub use self::unix::{
    pipe, set_nonblock, Awakener, EventedFd, Events, Io, Selector, TcpListener, TcpStream,
    UdpSocket,
};

#[cfg(unix)]
pub use self::unix::READY_ALL;

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub use self::windows::{
    Awakener, Binding, Events, Overlapped, Selector, TcpListener, TcpStream, UdpSocket,
};

#[cfg(windows)]
mod windows;

#[cfg(target_os = "fuchsia")]
pub use self::fuchsia::{
    set_nonblock, Awakener, EventedHandle, Events, Selector, TcpListener, TcpStream, UdpSocket,
};

#[cfg(windows)]
pub const READY_ALL: usize = 0;
