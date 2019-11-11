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
const DATA1_LEN: usize = 16;
const DATA2_LEN: usize = 14;
const DEFAULT_BUF_SIZE: usize = 64;
const TOKEN_1: Token = Token(0);
const TOKEN_2: Token = Token(1);

#[test]
fn is_send_and_sync() {
    assert_send::<UnixDatagram>();
    assert_sync::<UnixDatagram>();
}

#[test]
fn unix_datagram_smoke_unconnected() {
    let dir = assert_ok!(TempDir::new("unix"));
    let path_1 = dir.path().join("one");
    let path_2 = dir.path().join("two");

    let dg1 = assert_ok!(UnixDatagram::bind(&path_1));
    let dg2 = assert_ok!(UnixDatagram::bind(&path_2));
    smoke_test_unconnected(dg1, dg2);
}

#[test]
fn unix_datagram_smoke_connected() {
    let dir = assert_ok!(TempDir::new("unix"));
    let path_1 = dir.path().join("one");
    let path_2 = dir.path().join("two");

    let dg1 = assert_ok!(UnixDatagram::bind(&path_1));
    let dg2 = assert_ok!(UnixDatagram::bind(&path_2));

    assert_ok!(dg1.connect(&path_2));
    assert_ok!(dg2.connect(&path_1));
    smoke_test_connected(dg1, dg2);
}

#[test]
fn unix_datagram_smoke_unconnected_from_std() {
    let dir = assert_ok!(TempDir::new("unix"));
    let path_1 = dir.path().join("one");
    let path_2 = dir.path().join("two");

    let dg1 = assert_ok!(net::UnixDatagram::bind(&path_1));
    let dg2 = assert_ok!(net::UnixDatagram::bind(&path_2));

    assert_ok!(dg1.set_nonblocking(true));
    assert_ok!(dg2.set_nonblocking(true));

    let dg1 = UnixDatagram::from_std(dg1);
    let dg2 = UnixDatagram::from_std(dg2);
    smoke_test_unconnected(dg1, dg2);
}

#[test]
fn unix_datagram_smoke_connected_from_std() {
    let dir = assert_ok!(TempDir::new("unix"));
    let path_1 = dir.path().join("one");
    let path_2 = dir.path().join("two");

    let dg1 = assert_ok!(net::UnixDatagram::bind(&path_1));
    let dg2 = assert_ok!(net::UnixDatagram::bind(&path_2));

    assert_ok!(dg1.connect(&path_2));
    assert_ok!(dg2.connect(&path_1));

    assert_ok!(dg1.set_nonblocking(true));
    assert_ok!(dg2.set_nonblocking(true));

    let dg1 = UnixDatagram::from_std(dg1);
    let dg2 = UnixDatagram::from_std(dg2);
    smoke_test_connected(dg1, dg2);
}

#[test]
fn unix_datagram_connect() {
    let dir = assert_ok!(TempDir::new("unix"));
    let path_1 = dir.path().join("one");
    let path_2 = dir.path().join("two");

    let dg1 = assert_ok!(UnixDatagram::bind(&path_1));
    let dg1_local = assert_ok!(dg1.local_addr());
    let dg2 = assert_ok!(UnixDatagram::bind(&path_2));
    let dg2_local = assert_ok!(dg2.local_addr());

    assert_ok!(dg1.connect(dg1_local.as_pathname().expect("failed to get pathname")));
    assert_ok!(dg2.connect(dg2_local.as_pathname().expect("failed to get pathname")));
}

#[test]
fn unix_datagram_pair() {
    let (mut poll, mut events) = init_with_poll();

    let (dg1, dg2) = assert_ok!(UnixDatagram::pair());
    assert_ok!(poll
        .registry()
        .register(&dg1, TOKEN_1, Interests::READABLE | Interests::WRITABLE));
    assert_ok!(poll
        .registry()
        .register(&dg2, TOKEN_2, Interests::READABLE | Interests::WRITABLE));
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::WRITABLE),
            ExpectEvent::new(TOKEN_2, Interests::WRITABLE),
        ],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(dg1.recv(&mut buf));

    let wrote = assert_ok!(dg1.send(&DATA1));
    assert_eq!(wrote, DATA1_LEN);

    let read = assert_ok!(dg2.recv(&mut buf));
    assert_would_block(dg2.recv(&mut buf));
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);
    assert_eq!(read, wrote, "unequal reads and writes");

    let wrote = assert_ok!(dg2.send(&DATA2));
    assert_eq!(wrote, DATA2_LEN);

    let read = assert_ok!(dg1.recv(&mut buf));
    assert_eq!(read, DATA2_LEN);
    assert_eq!(&buf[..read], DATA2);
    assert_eq!(read, wrote, "unequal reads and writes");

    assert!(assert_ok!(dg1.take_error()).is_none());
    assert!(assert_ok!(dg2.take_error()).is_none());
}

#[test]
fn unix_datagram_try_clone() {
    let (mut poll, mut events) = init_with_poll();

    let dir = assert_ok!(TempDir::new("unix_datagram"));
    let path_1 = dir.path().join("one");
    let path_2 = dir.path().join("two");

    let dg1 = assert_ok!(UnixDatagram::bind(&path_1));
    let dg2 = assert_ok!(dg1.try_clone());
    assert_ne!(dg1.as_raw_fd(), dg2.as_raw_fd());

    let dg3 = assert_ok!(UnixDatagram::bind(&path_2));
    assert_ok!(dg3.connect(&path_1));

    assert_ok!(poll.registry().register(
        &dg1,
        TOKEN_1,
        Interests::READABLE.add(Interests::WRITABLE)
    ));
    assert_ok!(poll.registry().register(
        &dg2,
        TOKEN_2,
        Interests::READABLE.add(Interests::WRITABLE)
    ));
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::WRITABLE),
            ExpectEvent::new(TOKEN_2, Interests::WRITABLE),
        ],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(dg1.recv_from(&mut buf));

    assert_ok!(dg3.send(DATA1));
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::READABLE),
            ExpectEvent::new(TOKEN_2, Interests::READABLE),
        ],
    );

    let (read, from_addr_1) = assert_ok!(dg1.recv_from(&mut buf));
    assert_eq!(read, DATA1.len());
    assert_eq!(buf[..read], DATA1[..]);
    assert_eq!(
        from_addr_1.as_pathname().expect("failed to get pathname"),
        path_2
    );
    assert_would_block(dg2.recv_from(&mut buf));

    assert!(assert_ok!(dg1.take_error()).is_none());
    assert!(assert_ok!(dg2.take_error()).is_none());
    assert!(assert_ok!(dg3.take_error()).is_none());
}

#[test]
fn unix_datagram_shutdown_both() {
    let (mut poll, mut events) = init_with_poll();

    let dir = assert_ok!(TempDir::new("unix"));
    let path_1 = dir.path().join("one");
    let path_2 = dir.path().join("two");

    let dg1 = assert_ok!(UnixDatagram::bind(&path_1));
    let dg2 = assert_ok!(UnixDatagram::bind(&path_2));

    assert_ok!(poll.registry().register(
        &dg1,
        TOKEN_1,
        Interests::WRITABLE.add(Interests::READABLE)
    ));
    assert_ok!(poll.registry().register(
        &dg2,
        TOKEN_2,
        Interests::WRITABLE.add(Interests::READABLE)
    ));

    assert_ok!(dg1.connect(&path_2));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    let wrote = assert_ok!(dg1.send(DATA1));
    assert_eq!(wrote, DATA1_LEN);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_2, Interests::READABLE)],
    );

    assert_ok!(dg1.shutdown(Shutdown::Both));
    expect_readiness!(poll, events, is_write_closed);

    let err = assert_err!(dg1.send(DATA2));
    #[cfg(unix)]
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    #[cfg(window)]
    assert_eq!(err.kind(), io::ErrorKind::ConnectionAbroted);

    assert!(assert_ok!(dg1.take_error()).is_none());
}

#[test]
fn unix_datagram_register() {
    let (mut poll, mut events) = init_with_poll();

    let dir = assert_ok!(TempDir::new("unix"));
    let path = dir.path().join("any");

    let datagram = assert_ok!(UnixDatagram::bind(path));
    assert_ok!(poll
        .registry()
        .register(&datagram, TOKEN_1, Interests::READABLE));
    expect_no_events(&mut poll, &mut events);
}

#[test]
fn unix_datagram_reregister() {
    let (mut poll, mut events) = init_with_poll();

    let dir = assert_ok!(TempDir::new("unix"));
    let path_1 = dir.path().join("one");
    let path_2 = dir.path().join("two");

    let dg1 = assert_ok!(UnixDatagram::bind(&path_1));
    assert_ok!(poll.registry().register(&dg1, TOKEN_1, Interests::READABLE));

    let dg2 = assert_ok!(UnixDatagram::bind(&path_2));
    assert_ok!(dg2.connect(&path_1));
    assert_ok!(poll
        .registry()
        .reregister(&dg1, TOKEN_1, Interests::WRITABLE));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );
}

#[test]
fn unix_datagram_deregister() {
    let (mut poll, mut events) = init_with_poll();

    let dir = assert_ok!(TempDir::new("unix"));
    let path_1 = dir.path().join("one");
    let path_2 = dir.path().join("two");

    let dg1 = assert_ok!(UnixDatagram::bind(&path_1));
    assert_ok!(poll.registry().register(&dg1, TOKEN_1, Interests::WRITABLE));

    let dg2 = assert_ok!(UnixDatagram::bind(&path_2));
    assert_ok!(dg2.connect(&path_1));
    assert_ok!(poll.registry().deregister(&dg1));
    expect_no_events(&mut poll, &mut events);
}

fn smoke_test_unconnected(dg1: UnixDatagram, dg2: UnixDatagram) {
    let (mut poll, mut events) = init_with_poll();

    let addr_1 = assert_ok!(dg1.local_addr());
    let addr_2 = assert_ok!(dg2.local_addr());
    let path_1 = addr_1.as_pathname().expect("failed to get pathname");
    let path_2 = addr_2.as_pathname().expect("failed to get pathname");

    assert_ok!(poll.registry().register(
        &dg1,
        TOKEN_1,
        Interests::READABLE.add(Interests::WRITABLE)
    ));
    assert_ok!(poll.registry().register(
        &dg2,
        TOKEN_2,
        Interests::READABLE.add(Interests::WRITABLE)
    ));
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::WRITABLE),
            ExpectEvent::new(TOKEN_2, Interests::WRITABLE),
        ],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(dg1.recv_from(&mut buf));

    assert_ok!(dg1.send_to(DATA1, path_2));
    assert_ok!(dg2.send_to(DATA2, path_1));
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::READABLE),
            ExpectEvent::new(TOKEN_2, Interests::READABLE),
        ],
    );

    let (read, from_addr_1) = assert_ok!(dg1.recv_from(&mut buf));
    assert_eq!(read, DATA2.len());
    assert_eq!(buf[..read], DATA2[..]);
    assert_eq!(
        from_addr_1.as_pathname().expect("failed to get pathname"),
        path_2
    );

    let (read, from_addr_2) = assert_ok!(dg2.recv_from(&mut buf));
    assert_eq!(read, DATA1.len());
    assert_eq!(buf[..read], DATA1[..]);
    assert_eq!(
        from_addr_2.as_pathname().expect("failed to get pathname"),
        path_1
    );

    assert!(assert_ok!(dg1.take_error()).is_none());
    assert!(assert_ok!(dg2.take_error()).is_none());
}

fn smoke_test_connected(dg1: UnixDatagram, dg2: UnixDatagram) {
    let (mut poll, mut events) = init_with_poll();

    assert_ok!(poll.registry().register(
        &dg1,
        TOKEN_1,
        Interests::READABLE.add(Interests::WRITABLE)
    ));
    assert_ok!(poll.registry().register(
        &dg2,
        TOKEN_2,
        Interests::READABLE.add(Interests::WRITABLE)
    ));
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::WRITABLE),
            ExpectEvent::new(TOKEN_2, Interests::WRITABLE),
        ],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(dg1.recv(&mut buf));

    assert_ok!(dg1.send(DATA1));
    assert_ok!(dg2.send(DATA2));
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(TOKEN_1, Interests::READABLE),
            ExpectEvent::new(TOKEN_2, Interests::READABLE),
        ],
    );

    let read = assert_ok!(dg1.recv(&mut buf));
    assert_eq!(read, DATA2.len());
    assert_eq!(buf[..read], DATA2[..]);

    let read = assert_ok!(dg2.recv(&mut buf));
    assert_eq!(read, DATA1.len());
    assert_eq!(buf[..read], DATA1[..]);

    assert!(assert_ok!(dg1.take_error()).is_none());
    assert!(assert_ok!(dg2.take_error()).is_none());
}
