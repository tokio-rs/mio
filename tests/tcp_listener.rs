use std::io::Read;
use std::net::{self, SocketAddr};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::sync::{Arc, Barrier};
use std::thread;

use mio::net::TcpListener;
use mio::{Interests, Poll, Token};

mod util;

use util::{
    any_local_address, any_local_ipv6_address, assert_send, assert_sync, assert_would_block,
    expect_events, expect_no_events, init, init_with_poll, ExpectEvent,
};

const ID1: Token = Token(0);
const ID2: Token = Token(1);

#[test]
fn is_send_and_sync() {
    assert_send::<TcpListener>();
    assert_sync::<TcpListener>();
}

#[test]
fn tcp_listener() {
    smoke_test_tcp_listener(any_local_address());
}

#[test]
fn tcp_listener_ipv6() {
    smoke_test_tcp_listener(any_local_ipv6_address());
}

fn smoke_test_tcp_listener(addr: SocketAddr) {
    let (mut poll, mut events) = init_with_poll();

    let listener = TcpListener::bind(addr).unwrap();
    let address = listener.local_addr().unwrap();

    poll.registry()
        .register(&listener, ID1, Interests::READABLE)
        .expect("unable to register TCP listener");

    let barrier = Arc::new(Barrier::new(2));
    let thread_handle = start_connections(address, 1, barrier.clone());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );

    // Expect a single connection.
    let (mut stream, peer_address) = listener.accept().expect("unable to accept connection");
    assert!(peer_address.ip().is_loopback());
    assert_eq!(stream.peer_addr().unwrap(), peer_address);
    assert_eq!(stream.local_addr().unwrap(), address);

    // Expect the stream to be non-blocking.
    let mut buf = [0; 20];
    assert_would_block(stream.read(&mut buf));

    // Expect no more connections.
    assert_would_block(listener.accept());

    assert!(listener.take_error().unwrap().is_none());

    barrier.wait();
    thread_handle.join().expect("unable to join thread");
}

#[test]
fn ttl() {
    init();

    let listener = TcpListener::bind(any_local_address()).unwrap();

    const TTL: u32 = 10;
    listener.set_ttl(TTL).unwrap();
    assert_eq!(listener.ttl().unwrap(), TTL);
    assert!(listener.take_error().unwrap().is_none());
}

#[test]
#[cfg(unix)]
fn raw_fd() {
    init();

    let listener = TcpListener::bind(any_local_address()).unwrap();
    let address = listener.local_addr().unwrap();

    let raw_fd1 = listener.as_raw_fd();
    let raw_fd2 = listener.into_raw_fd();
    assert_eq!(raw_fd1, raw_fd2);

    let listener = unsafe { TcpListener::from_raw_fd(raw_fd2) };
    assert_eq!(listener.as_raw_fd(), raw_fd1);
    assert_eq!(listener.local_addr().unwrap(), address);
}

#[test]
fn registering() {
    let (mut poll, mut events) = init_with_poll();

    let stream = TcpListener::bind(any_local_address()).unwrap();

    poll.registry()
        .register(&stream, ID1, Interests::READABLE)
        .expect("unable to register TCP listener");

    expect_no_events(&mut poll, &mut events);

    // NOTE: more tests are done in the smoke tests above.
}

#[test]
fn reregister() {
    let (mut poll, mut events) = init_with_poll();

    let listener = TcpListener::bind(any_local_address()).unwrap();
    let address = listener.local_addr().unwrap();

    poll.registry()
        .register(&listener, ID1, Interests::READABLE)
        .unwrap();
    poll.registry()
        .reregister(&listener, ID2, Interests::READABLE)
        .unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let thread_handle = start_connections(address, 1, barrier.clone());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID2, Interests::READABLE)],
    );

    let (stream, peer_address) = listener.accept().expect("unable to accept connection");
    assert!(peer_address.ip().is_loopback());
    assert_eq!(stream.peer_addr().unwrap(), peer_address);
    assert_eq!(stream.local_addr().unwrap(), address);

    assert_would_block(listener.accept());

    assert!(listener.take_error().unwrap().is_none());

    barrier.wait();
    thread_handle.join().expect("unable to join thread");
}

#[test]
#[cfg_attr(windows, ignore = "deregister doesn't work, see #1073")]
fn deregister() {
    let (mut poll, mut events) = init_with_poll();

    let listener = TcpListener::bind(any_local_address()).unwrap();
    let address = listener.local_addr().unwrap();

    poll.registry()
        .register(&listener, ID1, Interests::READABLE)
        .unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let thread_handle = start_connections(address, 1, barrier.clone());

    poll.registry().deregister(&listener).unwrap();

    expect_no_events(&mut poll, &mut events);

    // Should still be able to accept the connection.
    let (stream, peer_address) = listener.accept().expect("unable to accept connection");
    assert!(peer_address.ip().is_loopback());
    assert_eq!(stream.peer_addr().unwrap(), peer_address);
    assert_eq!(stream.local_addr().unwrap(), address);

    assert_would_block(listener.accept());

    assert!(listener.take_error().unwrap().is_none());

    barrier.wait();
    thread_handle.join().expect("unable to join thread");
}

#[test]
#[cfg_attr(windows, ignore = "fails on Windows, see #1073")]
fn try_clone_same_poll() {
    let (mut poll, mut events) = init_with_poll();

    let listener1 = TcpListener::bind(any_local_address()).unwrap();
    let listener2 = listener1.try_clone().expect("unable to clone TCP listener");
    #[cfg(unix)]
    assert_ne!(listener1.as_raw_fd(), listener2.as_raw_fd());
    let address = listener1.local_addr().unwrap();
    assert_eq!(address, listener2.local_addr().unwrap());

    let barrier = Arc::new(Barrier::new(3));
    let thread_handle1 = start_connections(address, 1, barrier.clone());

    poll.registry()
        .register(&listener1, ID1, Interests::READABLE)
        .unwrap();
    poll.registry()
        .register(&listener2, ID2, Interests::READABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID1, Interests::READABLE),
            ExpectEvent::new(ID2, Interests::READABLE),
        ],
    );

    let (stream, peer_address) = listener1.accept().expect("unable to accept connection");
    assert!(peer_address.ip().is_loopback());
    assert_eq!(stream.peer_addr().unwrap(), peer_address);
    assert_eq!(stream.local_addr().unwrap(), address);

    let thread_handle2 = start_connections(address, 1, barrier.clone());

    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID1, Interests::READABLE),
            ExpectEvent::new(ID2, Interests::READABLE),
        ],
    );

    let (stream, peer_address) = listener2.accept().expect("unable to accept connection");
    assert!(peer_address.ip().is_loopback());
    assert_eq!(stream.peer_addr().unwrap(), peer_address);
    assert_eq!(stream.local_addr().unwrap(), address);

    assert_would_block(listener1.accept());
    assert_would_block(listener2.accept());

    assert!(listener1.take_error().unwrap().is_none());
    assert!(listener2.take_error().unwrap().is_none());

    barrier.wait();
    thread_handle1.join().expect("unable to join thread");
    thread_handle2.join().expect("unable to join thread");
}

#[test]
#[cfg_attr(windows, ignore = "fails on Windows, see #1073")]
fn try_clone_different_poll() {
    let (mut poll1, mut events) = init_with_poll();
    let mut poll2 = Poll::new().unwrap();

    let listener1 = TcpListener::bind(any_local_address()).unwrap();
    let listener2 = listener1.try_clone().expect("unable to clone TCP listener");
    #[cfg(unix)]
    assert_ne!(listener1.as_raw_fd(), listener2.as_raw_fd());
    let address = listener1.local_addr().unwrap();
    assert_eq!(address, listener2.local_addr().unwrap());

    let barrier = Arc::new(Barrier::new(3));
    let thread_handle1 = start_connections(address, 1, barrier.clone());

    poll1
        .registry()
        .register(&listener1, ID1, Interests::READABLE)
        .unwrap();
    poll2
        .registry()
        .register(&listener2, ID2, Interests::READABLE)
        .unwrap();

    expect_events(
        &mut poll1,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );
    expect_events(
        &mut poll2,
        &mut events,
        vec![ExpectEvent::new(ID2, Interests::READABLE)],
    );

    let (stream, peer_address) = listener1.accept().expect("unable to accept connection");
    assert!(peer_address.ip().is_loopback());
    assert_eq!(stream.peer_addr().unwrap(), peer_address);
    assert_eq!(stream.local_addr().unwrap(), address);

    let thread_handle2 = start_connections(address, 1, barrier.clone());

    expect_events(
        &mut poll1,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );
    expect_events(
        &mut poll2,
        &mut events,
        vec![ExpectEvent::new(ID2, Interests::READABLE)],
    );

    let (stream, peer_address) = listener2.accept().expect("unable to accept connection");
    assert!(peer_address.ip().is_loopback());
    assert_eq!(stream.peer_addr().unwrap(), peer_address);
    assert_eq!(stream.local_addr().unwrap(), address);

    assert_would_block(listener1.accept());
    assert_would_block(listener2.accept());

    assert!(listener1.take_error().unwrap().is_none());
    assert!(listener2.take_error().unwrap().is_none());

    barrier.wait();
    thread_handle1.join().expect("unable to join thread");
    thread_handle2.join().expect("unable to join thread");
}

/// Start `n_connections` connections to `address`. If a `barrier` is provided
/// it will wait on it after each connection is made before it is dropped.
fn start_connections(
    address: SocketAddr,
    n_connections: usize,
    barrier: Arc<Barrier>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for _ in 0..n_connections {
            let conn = net::TcpStream::connect(address).unwrap();
            barrier.wait();
            drop(conn);
        }
    })
}
