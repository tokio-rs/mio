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
pub use self::unix::READY_ALL;

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
    EventedHandle,
    Selector,
    TcpStream,
    TcpListener,
    UdpSocket,
    set_nonblock,
};

#[cfg(target_os = "fuchsia")]
pub mod fuchsia;

#[cfg(target_os = "redox")]
pub use self::redox::{
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


#[cfg(target_os = "redox")]
mod redox;

#[cfg(not(all(unix, not(any(target_os = "fuchsia", target_os = "redox")))))]
pub const READY_ALL: usize = 0;
