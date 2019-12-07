#![cfg(unix)]
#![cfg(all(feature = "os-poll", feature = "uds"))]

use mio::net::UnixListener;
use mio::{Interest, Poll, Token};
use std::io::{self, Read};
use std::os::unix::io::AsRawFd;
use std::os::unix::net;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use tempdir::TempDir;

#[macro_use]
mod util;
use util::{
    assert_send, assert_sync, assert_would_block, expect_events, expect_no_events, init_with_poll,
    ExpectEvent,
};

const DEFAULT_BUF_SIZE: usize = 64;
const TOKEN_1: Token = Token(0);
const TOKEN_2: Token = Token(1);

#[test]
fn unix_listener_send_and_sync() {
    assert_send::<UnixListener>();
    assert_sync::<UnixListener>();
}

#[test]
fn unix_listener_smoke() {
    #[allow(clippy::redundant_closure)]
    smoke_test(|path| UnixListener::bind(path));
}

#[test]
fn unix_listener_from_std() {
    smoke_test(|path| {
        let listener = net::UnixListener::bind(path).unwrap();
        // `std::os::unix::net::UnixStream`s are blocking by default, so make sure
        // it is in non-blocking mode before wrapping in a Mio equivalent.
        listener.set_nonblocking(true).unwrap();
        Ok(UnixListener::from_std(listener))
    })
}

#[test]
fn unix_listener_try_clone_same_poll() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(3));
    let dir = TempDir::new("unix_listener").unwrap();
    let path = dir.path().join("any");

    let mut listener1 = UnixListener::bind(&path).unwrap();
    let mut listener2 = listener1.try_clone().unwrap();
    assert_ne!(listener1.as_raw_fd(), listener2.as_raw_fd());

    let handle_1 = open_connections(path.clone(), 1, barrier.clone());
    poll.registry()
        .register(&mut listener1, TOKEN_1, Interest::READABLE)
        .unwrap();
    poll.registry()
        .register(&mut listener2, TOKEN_2, Interest::READABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interest::READABLE),
            ExpectEvent::new(TOKEN_2, Interest::READABLE),
        ],
    );

    listener1.accept().unwrap();

    let handle_2 = open_connections(path, 1, barrier.clone());
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interest::READABLE),
            ExpectEvent::new(TOKEN_2, Interest::READABLE),
        ],
    );

    listener2.accept().unwrap();

    assert_would_block(listener1.accept());
    assert_would_block(listener2.accept());

    assert!(listener1.take_error().unwrap().is_none());
    assert!(listener2.take_error().unwrap().is_none());

    barrier.wait();
    handle_1.join().unwrap();
    handle_2.join().unwrap();
}

#[test]
fn unix_listener_try_clone_different_poll() {
    let (mut poll1, mut events) = init_with_poll();
    let mut poll2 = Poll::new().unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let dir = TempDir::new("unix_listener").unwrap();
    let path = dir.path().join("any");

    let mut listener1 = UnixListener::bind(&path).unwrap();
    let mut listener2 = listener1.try_clone().unwrap();
    assert_ne!(listener1.as_raw_fd(), listener2.as_raw_fd());

    let handle_1 = open_connections(path.clone(), 1, barrier.clone());
    poll1
        .registry()
        .register(&mut listener1, TOKEN_1, Interest::READABLE)
        .unwrap();
    poll2
        .registry()
        .register(&mut listener2, TOKEN_2, Interest::READABLE)
        .unwrap();
    expect_events(
        &mut poll1,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );
    expect_events(
        &mut poll2,
        &mut events,
        vec![ExpectEvent::new(TOKEN_2, Interest::READABLE)],
    );

    listener1.accept().unwrap();

    let handle_2 = open_connections(path, 1, barrier.clone());
    expect_events(
        &mut poll1,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );
    expect_events(
        &mut poll2,
        &mut events,
        vec![ExpectEvent::new(TOKEN_2, Interest::READABLE)],
    );

    listener2.accept().unwrap();

    assert_would_block(listener1.accept());
    assert_would_block(listener2.accept());

    assert!(listener1.take_error().unwrap().is_none());
    assert!(listener2.take_error().unwrap().is_none());

    barrier.wait();
    handle_1.join().unwrap();
    handle_2.join().unwrap();
}

#[test]
fn unix_listener_local_addr() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let dir = TempDir::new("unix_listener").unwrap();
    let path = dir.path().join("any");

    let mut listener = UnixListener::bind(&path).unwrap();
    poll.registry()
        .register(
            &mut listener,
            TOKEN_1,
            Interest::WRITABLE.add(Interest::READABLE),
        )
        .unwrap();

    let handle = open_connections(path.clone(), 1, barrier.clone());
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );

    let (stream, expected_addr) = listener.accept().unwrap();
    assert_eq!(stream.local_addr().unwrap().as_pathname().unwrap(), &path);
    assert!(expected_addr.as_pathname().is_none());

    barrier.wait();
    handle.join().unwrap();
}

#[test]
fn unix_listener_register() {
    let (mut poll, mut events) = init_with_poll();
    let dir = TempDir::new("unix_listener").unwrap();

    let mut listener = UnixListener::bind(dir.path().join("any")).unwrap();
    poll.registry()
        .register(&mut listener, TOKEN_1, Interest::READABLE)
        .unwrap();
    expect_no_events(&mut poll, &mut events)
}

#[test]
fn unix_listener_reregister() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let dir = TempDir::new("unix_listener").unwrap();
    let path = dir.path().join("any");

    let mut listener = UnixListener::bind(&path).unwrap();
    poll.registry()
        .register(&mut listener, TOKEN_1, Interest::WRITABLE)
        .unwrap();

    let handle = open_connections(path, 1, barrier.clone());
    expect_no_events(&mut poll, &mut events);

    poll.registry()
        .reregister(&mut listener, TOKEN_1, Interest::READABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );

    barrier.wait();
    handle.join().unwrap();
}

#[test]
fn unix_listener_deregister() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let dir = TempDir::new("unix_listener").unwrap();
    let path = dir.path().join("any");

    let mut listener = UnixListener::bind(&path).unwrap();
    poll.registry()
        .register(&mut listener, TOKEN_1, Interest::READABLE)
        .unwrap();

    let handle = open_connections(path, 1, barrier.clone());

    poll.registry().deregister(&mut listener).unwrap();
    expect_no_events(&mut poll, &mut events);

    barrier.wait();
    handle.join().unwrap();
}

fn smoke_test<F>(new_listener: F)
where
    F: FnOnce(&Path) -> io::Result<UnixListener>,
{
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let dir = TempDir::new("unix_listener").unwrap();
    let path = dir.path().join("any");

    let mut listener = new_listener(&path).unwrap();
    poll.registry()
        .register(
            &mut listener,
            TOKEN_1,
            Interest::WRITABLE.add(Interest::READABLE),
        )
        .unwrap();
    expect_no_events(&mut poll, &mut events);

    let handle = open_connections(path, 1, barrier.clone());
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );

    let (mut stream, _) = listener.accept().unwrap();

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(stream.read(&mut buf));

    assert_would_block(listener.accept());
    assert!(listener.take_error().unwrap().is_none());

    barrier.wait();
    handle.join().unwrap();
}

fn open_connections(
    path: PathBuf,
    n_connections: usize,
    barrier: Arc<Barrier>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for _ in 0..n_connections {
            let conn = net::UnixStream::connect(path.clone()).unwrap();
            barrier.wait();
            drop(conn);
        }
    })
}
