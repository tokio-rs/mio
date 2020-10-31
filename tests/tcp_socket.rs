#![cfg(all(feature = "os-poll", feature = "tcp"))]

use mio::net::TcpSocket;

#[test]
fn is_send_and_sync() {
    fn is_send<T: Send>() {}
    fn is_sync<T: Sync>() {}

    is_send::<TcpSocket>();
    is_sync::<TcpSocket>();
}

#[test]
fn set_reuseaddr() {
    let addr = "127.0.0.1:0".parse().unwrap();

    let socket = TcpSocket::new_v4().unwrap();
    socket.set_reuseaddr(true).unwrap();
    assert!(socket.get_reuseaddr().unwrap());

    socket.bind(addr).unwrap();

    let _ = socket.listen(128).unwrap();
}

#[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
#[test]
fn set_reuseport() {
    let addr = "127.0.0.1:0".parse().unwrap();

    let socket = TcpSocket::new_v4().unwrap();
    socket.set_reuseport(true).unwrap();
    assert!(socket.get_reuseport().unwrap());

    socket.bind(addr).unwrap();

    let _ = socket.listen(128).unwrap();
}

#[test]
fn get_localaddr() {
    let expected_addr = "127.0.0.1:0".parse().unwrap();
    let socket = TcpSocket::new_v4().unwrap();

    //Windows doesn't support calling getsockname before calling `bind`
    #[cfg(not(windows))]
    assert_eq!("0.0.0.0:0", socket.get_localaddr().unwrap().to_string());

    socket.bind(expected_addr).unwrap();

    let actual_addr = socket.get_localaddr().unwrap();

    assert_eq!(expected_addr.ip(), actual_addr.ip());
    assert!(actual_addr.port() > 0);

    let _ = socket.listen(128).unwrap();
}

#[test]
fn send_buffer_size_roundtrips() {
    fn test(size: u32) {
        println!("testing send buffer size: {}", size);
        let socket = TcpSocket::new_v4().unwrap();
        socket.set_send_buffer_size(size).unwrap();
        // As per `man socket(7)`:
        // > Sets or gets the maximum socket send buffer in bytes.  The
        // > kernel doubles this value (to allow space for bookkeeping
        // > overhead) when it is set using setsockopt(2), and this doubled
        // > value is returned by getsockopt(2).
        assert_eq!(size * 2, socket.get_send_buffer_size().unwrap());
    }

    test(4096);
    test(65512);
}

#[test]
fn recv_buffer_size_roundtrips() {
    fn test(size: u32) {
        println!("testing recv buffer size: {}", size);
        let socket = TcpSocket::new_v4().unwrap();
        socket.set_recv_buffer_size(size).unwrap();
        // As per `man socket(7)`:
        // > Sets or gets the maximum socket receive buffer in bytes.  The
        // > kernel doubles this value (to allow space for bookkeeping
        // > overhead) when it is set using setsockopt(2), and this doubled
        // > value is returned by getsockopt(2).
        assert_eq!(size * 2, socket.get_recv_buffer_size().unwrap());
    }

    test(4096);
    test(65512);
}
