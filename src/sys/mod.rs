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
    set_nonblock,
};

#[cfg(all(unix, not(target_os="emscripten")))]
pub use self::unix::{
    pipe,
};

#[cfg(unix)]
#[cfg(feature = "with-deprecated")]
pub use self::unix::UnixSocket;

#[cfg(unix)]
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
