#[test]
#[cfg(unix)]
#[cfg(not(debug_assertions))]
fn assert_size() {
    use mio::net::*;
    use std::mem::size_of;

    // Without debug assertions enabled `TcpListener`, `TcpStream` and `UdpSocket` should have the
    // same size as the system specific socket, i.e. just a file descriptor on Unix platforms.
    assert_eq!(size_of::<TcpListener>(), size_of::<std::net::TcpListener>());
    assert_eq!(size_of::<TcpStream>(), size_of::<std::net::TcpStream>());
    assert_eq!(size_of::<UdpSocket>(), size_of::<std::net::UdpSocket>());
}
