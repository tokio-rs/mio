#![cfg(unix)]
#[macro_use]
mod util;

use mio::net::UnixListener;
use mio::{Interests, Poll, Token};
use std::io::{self, Read};
use std::os::unix::io::AsRawFd;
use std::os::unix::net;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use tempdir::TempDir;
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
        let listener = assert_ok!(net::UnixListener::bind(path));
        // `std::os::unix::net::UnixStream`s are blocking by default, so make sure
        // it is in non-blocking mode before wrapping in a Mio equivalent.
        assert_ok!(listener.set_nonblocking(true));
        Ok(UnixListener::from_std(listener))
    })
}

#[test]
fn unix_listener_try_clone_same_poll() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(3));
    let dir = assert_ok!(TempDir::new("unix_listener"));
    let path = dir.path().join("any");

    let listener1 = assert_ok!(UnixListener::bind(&path));
    let listener2 = assert_ok!(listener1.try_clone());
    assert_ne!(listener1.as_raw_fd(), listener2.as_raw_fd());

    let handle_1 = open_connections(path.clone(), 1, barrier.clone());
    assert_ok!(poll
        .registry()
        .register(&listener1, TOKEN_1, Interests::READABLE));
    assert_ok!(poll
        .registry()
        .register(&listener2, TOKEN_2, Interests::READABLE));
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::READABLE),
            ExpectEvent::new(TOKEN_2, Interests::READABLE),
        ],
    );

    assert_ok!(listener1.accept());

    let handle_2 = open_connections(path.clone(), 1, barrier.clone());
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::READABLE),
            ExpectEvent::new(TOKEN_2, Interests::READABLE),
        ],
    );

    assert_ok!(listener2.accept());

    assert_would_block(listener1.accept());
    assert_would_block(listener2.accept());

    assert!(assert_ok!(listener1.take_error()).is_none());
    assert!(assert_ok!(listener2.take_error()).is_none());

    barrier.wait();
    assert_ok!(handle_1.join());
    assert_ok!(handle_2.join());
}

#[test]
fn unix_listener_try_clone_different_poll() {
    let (mut poll1, mut events) = init_with_poll();
    let mut poll2 = assert_ok!(Poll::new());
    let barrier = Arc::new(Barrier::new(3));
    let dir = assert_ok!(TempDir::new("unix_listener"));
    let path = dir.path().join("any");

    let listener1 = assert_ok!(UnixListener::bind(&path));
    let listener2 = assert_ok!(listener1.try_clone());
    assert_ne!(listener1.as_raw_fd(), listener2.as_raw_fd());

    let handle_1 = open_connections(path.clone(), 1, barrier.clone());
    assert_ok!(poll1
        .registry()
        .register(&listener1, TOKEN_1, Interests::READABLE));
    assert_ok!(poll2
        .registry()
        .register(&listener2, TOKEN_2, Interests::READABLE));
    expect_events(
        &mut poll1,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );
    expect_events(
        &mut poll2,
        &mut events,
        vec![ExpectEvent::new(TOKEN_2, Interests::READABLE)],
    );

    assert_ok!(listener1.accept());

    let handle_2 = open_connections(path.clone(), 1, barrier.clone());
    expect_events(
        &mut poll1,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );
    expect_events(
        &mut poll2,
        &mut events,
        vec![ExpectEvent::new(TOKEN_2, Interests::READABLE)],
    );

    assert_ok!(listener2.accept());

    assert_would_block(listener1.accept());
    assert_would_block(listener2.accept());

    assert!(assert_ok!(listener1.take_error()).is_none());
    assert!(assert_ok!(listener2.take_error()).is_none());

    barrier.wait();
    assert_ok!(handle_1.join());
    assert_ok!(handle_2.join());
}

#[test]
fn unix_listener_local_addr() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let dir = assert_ok!(TempDir::new("unix_listener"));
    let path = dir.path().join("any");

    let listener = assert_ok!(UnixListener::bind(&path));
    assert_ok!(poll.registry().register(
        &listener,
        TOKEN_1,
        Interests::WRITABLE.add(Interests::READABLE)
    ));

    let handle = open_connections(path.clone(), 1, barrier.clone());
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );

    let (stream, expected_addr) = assert_ok!(listener.accept());
    assert_eq!(
        assert_ok!(stream.local_addr()).as_pathname().unwrap(),
        &path
    );
    assert!(expected_addr.as_pathname().is_none());

    barrier.wait();
    assert_ok!(handle.join());
}

#[test]
fn unix_listener_register() {
    let (mut poll, mut events) = init_with_poll();
    let dir = assert_ok!(TempDir::new("unix_listener"));

    let listener = assert_ok!(UnixListener::bind(dir.path().join("any")));
    assert_ok!(poll
        .registry()
        .register(&listener, TOKEN_1, Interests::READABLE));
    expect_no_events(&mut poll, &mut events)
}

#[test]
fn unix_listener_reregister() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let dir = assert_ok!(TempDir::new("unix_listener"));
    let path = dir.path().join("any");

    let listener = assert_ok!(UnixListener::bind(&path));
    assert_ok!(poll
        .registry()
        .register(&listener, TOKEN_1, Interests::WRITABLE));

    let handle = open_connections(path.clone(), 1, barrier.clone());
    expect_no_events(&mut poll, &mut events);

    assert_ok!(poll
        .registry()
        .reregister(&listener, TOKEN_1, Interests::READABLE));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );

    barrier.wait();
    assert_ok!(handle.join());
}

#[test]
fn unix_listener_deregister() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let dir = assert_ok!(TempDir::new("unix_listener"));
    let path = dir.path().join("any");

    let listener = assert_ok!(UnixListener::bind(&path));
    assert_ok!(poll
        .registry()
        .register(&listener, TOKEN_1, Interests::READABLE));

    let handle = open_connections(path.clone(), 1, barrier.clone());

    assert_ok!(poll.registry().deregister(&listener));
    expect_no_events(&mut poll, &mut events);

    barrier.wait();
    assert_ok!(handle.join());
}

fn smoke_test<F>(new_listener: F)
where
    F: FnOnce(&Path) -> io::Result<UnixListener>,
{
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let dir = assert_ok!(TempDir::new("unix_listener"));
    let path = dir.path().join("any");

    let listener = assert_ok!(new_listener(&path));
    assert_ok!(poll.registry().register(
        &listener,
        TOKEN_1,
        Interests::WRITABLE.add(Interests::READABLE)
    ));
    expect_no_events(&mut poll, &mut events);

    let handle = open_connections(path.clone(), 1, barrier.clone());
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );

    let (mut stream, _) = assert_ok!(listener.accept());

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(stream.read(&mut buf));

    assert_would_block(listener.accept());
    assert!(assert_ok!(listener.take_error()).is_none());

    barrier.wait();
    assert_ok!(handle.join());
}

fn open_connections(
    path: PathBuf,
    n_connections: usize,
    barrier: Arc<Barrier>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for _ in 0..n_connections {
            let conn = assert_ok!(net::UnixStream::connect(path.clone()));
            barrier.wait();
            drop(conn);
        }
    })
}
