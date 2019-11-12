#![cfg(unix)]
#[macro_use]
mod util;

use log::warn;
use mio::net::UnixDatagram;
use mio::{Interests, Token};
use std::io;
use std::net::Shutdown;
use std::os::unix::io::AsRawFd;
use std::os::unix::net;
use std::time::Duration;
use tempdir::TempDir;
use util::{
    assert_send, assert_sync, assert_would_block, expect_events, expect_no_events, init_with_poll,
    ExpectEvent,
};

const DATA1: &[u8] = b"Hello same host!";
const DATA2: &[u8] = b"Why hello mio!";
const DATA1_LEN: usize = DATA1.len();
const DATA2_LEN: usize = DATA2.len();
const DEFAULT_BUF_SIZE: usize = 64;
const TEST_DIR: &str = "mio_unix_datagram_tests";
const TOKEN_1: Token = Token(0);
const TOKEN_2: Token = Token(1);
const TOKEN_3: Token = Token(2);

#[test]
fn is_send_and_sync() {
    assert_send::<UnixDatagram>();
    assert_sync::<UnixDatagram>();
}

#[test]
fn unix_datagram_smoke_unconnected() {
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path1 = dir.path().join("one");
    let path2 = dir.path().join("two");

    let datagram1 = UnixDatagram::bind(&path1).unwrap();
    let datagram2 = UnixDatagram::bind(&path2).unwrap();
    smoke_test_unconnected(datagram1, datagram2);
}

#[test]
fn unix_datagram_smoke_connected() {
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path1 = dir.path().join("one");
    let path2 = dir.path().join("two");

    let datagram1 = UnixDatagram::bind(&path1).unwrap();
    let datagram2 = UnixDatagram::bind(&path2).unwrap();

    datagram1.connect(&path2).unwrap();
    datagram2.connect(&path1).unwrap();
    smoke_test_connected(datagram1, datagram2);
}

#[test]
fn unix_datagram_smoke_unconnected_from_std() {
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path1 = dir.path().join("one");
    let path2 = dir.path().join("two");

    let datagram1 = net::UnixDatagram::bind(&path1).unwrap();
    let datagram2 = net::UnixDatagram::bind(&path2).unwrap();

    datagram1.set_nonblocking(true).unwrap();
    datagram2.set_nonblocking(true).unwrap();

    let datagram1 = UnixDatagram::from_std(datagram1);
    let datagram2 = UnixDatagram::from_std(datagram2);
    smoke_test_unconnected(datagram1, datagram2);
}

#[test]
fn unix_datagram_smoke_connected_from_std() {
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path1 = dir.path().join("one");
    let path2 = dir.path().join("two");

    let datagram1 = net::UnixDatagram::bind(&path1).unwrap();
    let datagram2 = net::UnixDatagram::bind(&path2).unwrap();

    datagram1.connect(&path2).unwrap();
    datagram2.connect(&path1).unwrap();

    datagram1.set_nonblocking(true).unwrap();
    datagram2.set_nonblocking(true).unwrap();

    let datagram1 = UnixDatagram::from_std(datagram1);
    let datagram2 = UnixDatagram::from_std(datagram2);
    smoke_test_connected(datagram1, datagram2);
}

#[test]
fn unix_datagram_connect() {
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path1 = dir.path().join("one");
    let path2 = dir.path().join("two");

    let datagram1 = UnixDatagram::bind(&path1).unwrap();
    let datagram1_local = datagram1.local_addr().unwrap();
    let datagram2 = UnixDatagram::bind(&path2).unwrap();
    let datagram2_local = datagram2.local_addr().unwrap();

    datagram1
        .connect(
            datagram1_local
                .as_pathname()
                .expect("failed to get pathname"),
        )
        .unwrap();
    datagram2
        .connect(
            datagram2_local
                .as_pathname()
                .expect("failed to get pathname"),
        )
        .unwrap();
}

#[test]
fn unix_datagram_pair() {
    let (mut poll, mut events) = init_with_poll();

    let (datagram1, datagram2) = UnixDatagram::pair().unwrap();
    poll.registry()
        .register(
            &datagram1,
            TOKEN_1,
            Interests::READABLE | Interests::WRITABLE,
        )
        .unwrap();
    poll.registry()
        .register(
            &datagram2,
            TOKEN_2,
            Interests::READABLE | Interests::WRITABLE,
        )
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::WRITABLE),
            ExpectEvent::new(TOKEN_2, Interests::WRITABLE),
        ],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(datagram1.recv(&mut buf));
    assert_would_block(datagram2.recv(&mut buf));

    let wrote1 = datagram1.send(&DATA1).unwrap();
    assert_eq!(wrote1, DATA1_LEN);
    let wrote2 = datagram2.send(&DATA2).unwrap();
    assert_eq!(wrote2, DATA2_LEN);
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::READABLE),
            ExpectEvent::new(TOKEN_2, Interests::READABLE),
        ],
    );

    let read = datagram2.recv(&mut buf).unwrap();
    assert_would_block(datagram2.recv(&mut buf));
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);
    assert_eq!(read, wrote1, "unequal reads and writes");

    let read = datagram1.recv(&mut buf).unwrap();
    assert_eq!(read, DATA2_LEN);
    assert_eq!(&buf[..read], DATA2);
    assert_eq!(read, wrote2, "unequal reads and writes");

    assert!(datagram1.take_error().unwrap().is_none());
    assert!(datagram2.take_error().unwrap().is_none());
}

#[test]
fn unix_datagram_try_clone() {
    let (mut poll, mut events) = init_with_poll();
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path1 = dir.path().join("one");
    let path2 = dir.path().join("two");

    let datagram1 = UnixDatagram::bind(&path1).unwrap();
    let datagram2 = datagram1.try_clone().unwrap();
    assert_ne!(datagram1.as_raw_fd(), datagram2.as_raw_fd());

    let datagram3 = UnixDatagram::bind(&path2).unwrap();
    datagram3.connect(&path1).unwrap();

    poll.registry()
        .register(&datagram1, TOKEN_1, Interests::READABLE)
        .unwrap();
    poll.registry()
        .register(&datagram2, TOKEN_2, Interests::READABLE)
        .unwrap();
    poll.registry()
        .register(
            &datagram3,
            TOKEN_3,
            Interests::READABLE.add(Interests::WRITABLE),
        )
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_3, Interests::WRITABLE)],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(datagram1.recv_from(&mut buf));
    assert_would_block(datagram2.recv_from(&mut buf));
    assert_would_block(datagram3.recv_from(&mut buf));

    datagram3.send(DATA1).unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::READABLE),
            ExpectEvent::new(TOKEN_2, Interests::READABLE),
        ],
    );

    let (read, from_addr1) = datagram1.recv_from(&mut buf).unwrap();
    assert_eq!(read, DATA1_LEN);
    assert_eq!(buf[..read], DATA1[..]);
    assert_eq!(
        from_addr1.as_pathname().expect("failed to get pathname"),
        path2
    );
    assert_would_block(datagram2.recv_from(&mut buf));

    assert!(datagram1.take_error().unwrap().is_none());
    assert!(datagram2.take_error().unwrap().is_none());
    assert!(datagram3.take_error().unwrap().is_none());
}

#[test]
fn unix_datagram_shutdown() {
    let (mut poll, mut events) = init_with_poll();
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path1 = dir.path().join("one");
    let path2 = dir.path().join("two");

    let datagram1 = UnixDatagram::bind(&path1).unwrap();
    let datagram2 = UnixDatagram::bind(&path2).unwrap();

    poll.registry()
        .register(
            &datagram1,
            TOKEN_1,
            Interests::WRITABLE.add(Interests::READABLE),
        )
        .unwrap();
    poll.registry()
        .register(
            &datagram2,
            TOKEN_2,
            Interests::WRITABLE.add(Interests::READABLE),
        )
        .unwrap();

    datagram1.connect(&path2).unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    let wrote = datagram1.send(DATA1).unwrap();
    assert_eq!(wrote, DATA1_LEN);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_2, Interests::READABLE)],
    );

    datagram1.shutdown(Shutdown::Read).unwrap();
    expect_readiness!(poll, events, is_read_closed);

    datagram1.shutdown(Shutdown::Write).unwrap();
    expect_readiness!(poll, events, is_write_closed);

    let err = datagram1.send(DATA2).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);

    assert!(datagram1.take_error().unwrap().is_none());
}

#[test]
fn unix_datagram_register() {
    let (mut poll, mut events) = init_with_poll();
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path = dir.path().join("any");

    let datagram = UnixDatagram::bind(path).unwrap();
    poll.registry()
        .register(&datagram, TOKEN_1, Interests::READABLE)
        .unwrap();
    expect_no_events(&mut poll, &mut events);
}

#[test]
fn unix_datagram_reregister() {
    let (mut poll, mut events) = init_with_poll();
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path1 = dir.path().join("one");
    let path2 = dir.path().join("two");

    let datagram1 = UnixDatagram::bind(&path1).unwrap();
    poll.registry()
        .register(&datagram1, TOKEN_1, Interests::READABLE)
        .unwrap();

    let datagram2 = UnixDatagram::bind(&path2).unwrap();
    datagram2.connect(&path1).unwrap();
    poll.registry()
        .reregister(&datagram1, TOKEN_1, Interests::WRITABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );
}

#[test]
fn unix_datagram_deregister() {
    let (mut poll, mut events) = init_with_poll();
    let dir = TempDir::new(TEST_DIR).unwrap();
    let path1 = dir.path().join("one");
    let path2 = dir.path().join("two");

    let datagram1 = UnixDatagram::bind(&path1).unwrap();
    poll.registry()
        .register(&datagram1, TOKEN_1, Interests::WRITABLE)
        .unwrap();

    let datagram2 = UnixDatagram::bind(&path2).unwrap();
    datagram2.connect(&path1).unwrap();
    poll.registry().deregister(&datagram1).unwrap();
    expect_no_events(&mut poll, &mut events);
}

fn smoke_test_unconnected(datagram1: UnixDatagram, datagram2: UnixDatagram) {
    let (mut poll, mut events) = init_with_poll();

    let addr1 = datagram1.local_addr().unwrap();
    let addr2 = datagram2.local_addr().unwrap();
    let path1 = addr1.as_pathname().expect("failed to get pathname");
    let path2 = addr2.as_pathname().expect("failed to get pathname");

    poll.registry()
        .register(
            &datagram1,
            TOKEN_1,
            Interests::READABLE.add(Interests::WRITABLE),
        )
        .unwrap();
    poll.registry()
        .register(
            &datagram2,
            TOKEN_2,
            Interests::READABLE.add(Interests::WRITABLE),
        )
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::WRITABLE),
            ExpectEvent::new(TOKEN_2, Interests::WRITABLE),
        ],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(datagram1.recv_from(&mut buf));
    assert_would_block(datagram2.recv_from(&mut buf));

    datagram1.send_to(DATA1, path2).unwrap();
    datagram2.send_to(DATA2, path1).unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::READABLE),
            ExpectEvent::new(TOKEN_2, Interests::READABLE),
        ],
    );

    let (read, from_addr1) = datagram1.recv_from(&mut buf).unwrap();
    assert_eq!(read, DATA2_LEN);
    assert_eq!(buf[..read], DATA2[..]);
    assert_eq!(
        from_addr1.as_pathname().expect("failed to get pathname"),
        path2
    );

    let (read, from_addr2) = datagram2.recv_from(&mut buf).unwrap();
    assert_eq!(read, DATA1_LEN);
    assert_eq!(buf[..read], DATA1[..]);
    assert_eq!(
        from_addr2.as_pathname().expect("failed to get pathname"),
        path1
    );

    assert!(datagram1.take_error().unwrap().is_none());
    assert!(datagram2.take_error().unwrap().is_none());
}

fn smoke_test_connected(datagram1: UnixDatagram, datagram2: UnixDatagram) {
    let (mut poll, mut events) = init_with_poll();

    let local_addr1 = datagram1.local_addr().unwrap();
    let peer_addr1 = datagram1.peer_addr().unwrap();
    let local_addr2 = datagram2.local_addr().unwrap();
    let peer_addr2 = datagram2.peer_addr().unwrap();
    assert_eq!(
        local_addr1.as_pathname().expect("failed to get pathname"),
        peer_addr2.as_pathname().expect("failed to get pathname")
    );
    assert_eq!(
        local_addr2.as_pathname().expect("failed to get pathname"),
        peer_addr1.as_pathname().expect("failed to get pathname")
    );

    poll.registry()
        .register(
            &datagram1,
            TOKEN_1,
            Interests::READABLE.add(Interests::WRITABLE),
        )
        .unwrap();
    poll.registry()
        .register(
            &datagram2,
            TOKEN_2,
            Interests::READABLE.add(Interests::WRITABLE),
        )
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::WRITABLE),
            ExpectEvent::new(TOKEN_2, Interests::WRITABLE),
        ],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(datagram1.recv(&mut buf));
    assert_would_block(datagram2.recv(&mut buf));

    datagram1.send(DATA1).unwrap();
    datagram2.send(DATA2).unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::READABLE),
            ExpectEvent::new(TOKEN_2, Interests::READABLE),
        ],
    );

    let read = datagram1.recv(&mut buf).unwrap();
    assert_eq!(read, DATA2_LEN);
    assert_eq!(buf[..read], DATA2[..]);

    let read = datagram2.recv(&mut buf).unwrap();
    assert_eq!(read, DATA1_LEN);
    assert_eq!(buf[..read], DATA1[..]);

    assert!(datagram1.take_error().unwrap().is_none());
    assert!(datagram2.take_error().unwrap().is_none());
}
