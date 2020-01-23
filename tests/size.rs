#[test]
#[cfg(any(
    all(
        any(target_os = "linux", target_os = "android", target_os = "illumos",),
        feature = "os-epoll",
    ),
    all(
        any(
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "ios",
            target_os = "macos",
            target_os = "netbsd",
            target_os = "openbsd",
        ),
        feature = "os-kqueue"
    )
))]
#[cfg(unix)]
#[cfg(not(debug_assertions))]
fn assert_size() {
    // Without debug assertions enabled `TcpListener`, `TcpStream` and `UdpSocket` should have the
    // same size as the system specific socket, i.e. just a file descriptor on Unix platforms unless I/O selector backend uses POSIX poll(2).
    {
        use mio::net::*;
        use std::mem::size_of;
        assert_eq!(size_of::<TcpListener>(), size_of::<std::net::TcpListener>());
        assert_eq!(size_of::<TcpStream>(), size_of::<std::net::TcpStream>());
        assert_eq!(size_of::<UdpSocket>(), size_of::<std::net::UdpSocket>());
    }
}
