// SelectorId is only used debug assertions are enabled.
#![cfg(debug_assertions)]

use std::io;

use mio::net::{TcpListener, TcpStream, UdpSocket};
use mio::{Interests, Poll, Token};

use crate::util::localhost;

#[test]
fn tcp() {
    let addr = localhost();
    let listener = TcpListener::bind(addr).unwrap();

    let poll1 = Poll::new().unwrap();
    poll1
        .registry()
        .register(
            &listener,
            Token(0),
            Interests::READABLE | Interests::WRITABLE,
        )
        .unwrap();

    let poll2 = Poll::new().unwrap();

    // Try registering the same socket with the initial one
    let res = poll2.registry().register(
        &listener,
        Token(0),
        Interests::READABLE | Interests::WRITABLE,
    );
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);

    // Try cloning the socket and registering it again
    let listener2 = listener.try_clone().unwrap();
    let res = poll2.registry().register(
        &listener2,
        Token(0),
        Interests::READABLE | Interests::WRITABLE,
    );
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);

    // Try the stream
    let stream = TcpStream::connect(addr).unwrap();

    poll1
        .registry()
        .register(&stream, Token(1), Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    let res =
        poll2
            .registry()
            .register(&stream, Token(1), Interests::READABLE | Interests::WRITABLE);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);

    // Try cloning the socket and registering it again
    let stream2 = stream.try_clone().unwrap();
    let res = poll2.registry().register(
        &stream2,
        Token(1),
        Interests::READABLE | Interests::WRITABLE,
    );
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);
}

#[test]
fn udp() {
    let addr = localhost();
    let socket = UdpSocket::bind(addr).unwrap();

    let poll1 = Poll::new().unwrap();
    poll1
        .registry()
        .register(&socket, Token(0), Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    let poll2 = Poll::new().unwrap();

    // Try registering the same socket with the initial one
    let res =
        poll2
            .registry()
            .register(&socket, Token(0), Interests::READABLE | Interests::WRITABLE);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);

    // Try cloning the socket and registering it again
    let socket2 = socket.try_clone().unwrap();
    let res = poll2.registry().register(
        &socket2,
        Token(0),
        Interests::READABLE | Interests::WRITABLE,
    );
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);
}
