#[cfg(all(unix, not(target_os = "fuchsia")))]
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
};

#[cfg(all(unix, not(target_os = "fuchsia")))]
#[cfg(feature = "with-deprecated")]
pub use self::unix::UnixSocket;

#[cfg(all(unix, not(target_os = "fuchsia")))]
pub mod unix;

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
};

#[cfg(windows)]
mod windows;

#[cfg(target_os = "fuchsia")]
pub use self::fuchsia::{
    Awakener,
    Events,
    Selector,
    TcpStream,
    TcpListener,
    UdpSocket,
};

#[cfg(target_os = "fuchsia")]
mod fuchsia;
