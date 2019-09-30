#[macro_use]
mod util;

use mio::net::{UnixListener, UnixStream};
use mio::{Interests, Token};
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
#[cfg(unix)]
use std::os::unix::net::SocketAddr;
use std::sync::mpsc::channel;
use std::thread;
use tempdir::TempDir;
use util::{
    assert_send, assert_sync, assert_would_block, expect_events, init_with_poll, ExpectEvent,
    TryRead, TryWrite,
};

const DATA1: &[u8] = b"Hello world!";
const DATA2: &[u8] = b"Hello mars!";

const DATA1_LEN: usize = 12;
const DATA2_LEN: usize = 11;

const LOCAL: Token = Token(0);
const REMOTE: Token = Token(1);

#[test]
fn is_send_and_sync() {
    assert_send::<UnixListener>();
    assert_sync::<UnixListener>();

    assert_send::<UnixStream>();
    assert_sync::<UnixStream>();
}

#[test]
fn accept() {
    let (mut poll, mut events) = init_with_poll();

    let dir = assert_ok!(TempDir::new("uds"));
    let path = dir.path().join("path");

    let remote = assert_ok!(UnixListener::bind(path));
    let addr = assert_ok!(remote.local_addr());
    let bp = addr.as_pathname().expect("not a pathname");

    let bound_path = bp.to_owned();
    let handle = thread::spawn(move || {
        assert_ok!(UnixStream::connect(bound_path));
    });

    assert_ok!(poll
        .registry()
        .register(&remote, REMOTE, Interests::READABLE));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(REMOTE, Interests::READABLE)],
    );

    assert_ok!(remote.accept());
    assert_would_block(remote.accept());
    assert_ok!(handle.join());
}

#[test]
fn connect() {
    let (mut poll, mut events) = init_with_poll();

    let dir = assert_ok!(TempDir::new("uds"));
    let path = dir.path().join("foo");

    let remote = assert_ok!(UnixListener::bind(path.clone()));
    let local = assert_ok!(UnixStream::connect(path));

    let (sender_1, receiver_1) = channel();
    let (sender_2, receiver_2) = channel();

    let handle = thread::spawn(move || {
        let (stream, _) = assert_ok!(remote.accept());
        assert_ok!(receiver_1.recv());
        drop(stream);
        assert_ok!(sender_2.send(()));
    });

    assert_ok!(poll
        .registry()
        .register(&local, LOCAL, Interests::READABLE | Interests::WRITABLE));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::WRITABLE)],
    );

    assert_ok!(sender_1.send(()));
    assert_ok!(receiver_2.recv());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
    );
    assert_ok!(handle.join());
}

#[test]
fn read() {
    const N: usize = 16 * 1024 * 1024;

    let (mut poll, mut events) = init_with_poll();

    let dir = assert_ok!(TempDir::new("uds"));
    let path = dir.path().join("foo");

    let remote = assert_ok!(UnixListener::bind(path.clone()));
    let mut local = assert_ok!(UnixStream::connect(path));

    let handle = thread::spawn(move || {
        let (mut stream, _) = assert_ok!(remote.accept());
        let b = [0; 1024];
        let mut written = 0;
        while written < N {
            if let Some(amount) = assert_ok!(stream.try_write(&b)) {
                written += amount;
            }
        }
    });

    assert_ok!(poll.registry().register(&local, LOCAL, Interests::READABLE));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
    );

    let mut b = [0; 1024];
    let mut read = 0;
    while read < N {
        if let Some(amount) = assert_ok!(local.try_read(&mut b)) {
            read += amount;
        }
    }

    assert_ok!(handle.join());
}

// #[test]
// fn smoke_test_unix_stream() {
//     let (mut poll, mut events) = init_with_poll();

//     let dir = assert_ok!(TempDir::new("uds"));
//     let path = dir.path().join("foo");

//     let remote = assert_ok!(UnixListener::bind(path.clone()));
//     let mut local = assert_ok!(UnixStream::connect(path.clone()));

//     let handle = echo_listener(1, remote);

//     assert_ok!(poll.registry().register(
//         &local,
//         LOCAL,
//         Interests::WRITABLE.add(Interests::READABLE)
//     ));

//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(LOCAL, Interests::WRITABLE)],
//     );

//     let mut buf = [0; 16];
//     assert_would_block(local.read(&mut buf));

//     // NOTE: the call to `peer_addr` must happen after we received a writable
//     // event as the stream might not yet be connected.
//     // assert_eq!(local.peer_addr().unwrap(), addr);
//     // assert!(local.local_addr().unwrap().ip().is_loopback());

//     let n = local.write(&DATA1).expect("unable to write to stream");
//     assert_eq!(n, DATA1.len());

//     local.flush().unwrap();

//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
//     );

//     let n = local.read(&mut buf).expect("unable to read from stream");
//     assert_eq!(n, DATA1.len());
//     assert_eq!(&buf[..n], DATA1);

//     assert!(local.take_error().unwrap().is_none());

//     let bufs = [IoSlice::new(&DATA1), IoSlice::new(&DATA2)];
//     let n = local
//         .write_vectored(&bufs)
//         .expect("unable to write vectored to stream");
//     assert_eq!(n, DATA1.len() + DATA2.len());

//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
//     );

//     let mut buf1 = [1; DATA1_LEN];
//     let mut buf2 = [2; DATA2_LEN + 1];
//     let mut bufs = [IoSliceMut::new(&mut buf1), IoSliceMut::new(&mut buf2)];
//     let n = local
//         .read_vectored(&mut bufs)
//         .expect("unable to read vectored from stream");
//     assert_eq!(n, DATA1.len() + DATA2.len());
//     assert_eq!(&buf1, DATA1);
//     assert_eq!(&buf2[..DATA2.len()], DATA2);
//     assert_eq!(buf2[DATA2.len()], 2); // Last byte should be unchanged.

//     // Close the connection to allow the listener to shutdown.
//     drop(local);

//     handle.join().expect("unable to join thread");
// }

// fn echo_listener(connections: usize, remote: UnixListener) -> thread::JoinHandle<()> {
//     thread::spawn(move || {
//         let mut buf = [0; 128];
//         for _ in 0..connections {
//             let (mut local, _) = assert_ok!(remote.accept());

//             // On Linux based system it will cause a connection reset
//             // error when the reading side of the peer connection is
//             // shutdown, we don't consider it an actual here.
//             // ...
//             // Or we shouldn't
//             loop {
//                 let read = assert_ok!(local.try_read(&mut buf));
//                 if read == 0 {
//                     break;
//                 }
//                 let wrote = assert_ok!(local.write(&buf[..read]));
//                 assert_eq!(read, wrote, "short write");
//             }
//         }
//     })
// }

// fn _make_listener(
//     connections: usize,
//     _barrier: Option<()>,
// ) -> (thread::JoinHandle<()>, SocketAddr) {
//     let (sender, receiver) = channel();
//     let handle = thread::spawn(move || {
//         let dir = assert_ok!(TempDir::new("uds"));
//         let path = dir.path().join("path");
//         let remote = assert_ok!(UnixListener::bind(path));
//         let local_addr = assert_ok!(remote.local_addr());
//         assert_ok!(sender.send(local_addr));

//         for _ in 0..connections {
//             let (stream, _) = assert_ok!(remote.accept());
//             drop(stream);
//         }
//     });
//     (handle, assert_ok!(receiver.recv()))
// }
