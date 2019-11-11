#![cfg(unix)]
#[macro_use]
mod util;

use log::warn;
use mio::net::UnixListener;
use mio::unix::SocketAddr;
use mio::{Interests, Token};
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::Shutdown;
use std::os::unix::net;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempdir::TempDir;
use util::{
    assert_send, assert_sync, assert_would_block, expect_events, expect_no_events, init_with_poll,
    ExpectEvent, TryRead, TryWrite,
};

const DATA1: &[u8] = b"Hello same host!";
const DATA2: &[u8] = b"Why hello mio!";

const DEFAULT_BUF_SIZE: usize = 64;
const DATA1_LEN: usize = 16;
const DATA2_LEN: usize = 14;

const TOKEN_1: Token = Token(0);
const TOKEN_2: Token = Token(1);

#[test]
fn uds_listener_send_and_sync() {
    assert_send::<UnixListener>();
    assert_sync::<UnixListener>();
}

#[test]
fn uds_listener_smoke() {
    #[allow(clippy::redundant_closure)]
    smoke_test(|path| UnixListener::bind(path));
}

// #[test]
// fn uds_stream_connect() {
//     let (mut poll, mut events) = init_with_poll();
//     let dir = assert_ok!(TempDir::new("uds"));
//     let path = dir.path().join("foo");
//     let listener = assert_ok!(net::UnixListener::bind(path.clone()));
//     let stream = assert_ok!(UnixStream::connect(path));

//     let barrier = Arc::new(Barrier::new(2));

//     let barrier_clone = barrier.clone();
//     let handle = thread::spawn(move || {
//         let (stream, _) = assert_ok!(listener.accept());
//         barrier_clone.wait();
//         drop(stream);
//     });

//     assert_ok!(poll.registry().register(
//         &stream,
//         TOKEN_1,
//         Interests::READABLE | Interests::WRITABLE
//     ));
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
//     );

//     barrier.wait();
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
//     );

//     assert_ok!(handle.join());
// }

// #[test]
// fn uds_stream_from_std() {
//     smoke_test(|path| {
//         let local = assert_ok!(net::UnixStream::connect(path));
//         // `std::os::unix::net::UnixStream`s are blocking by default, so make sure
//         // it is in non-blocking mode before wrapping in a Mio equivalent.
//         assert_ok!(local.set_nonblocking(true));
//         Ok(UnixStream::from_std(local))
//     })
// }

// #[test]
// fn uds_stream_pair() {
//     let (mut poll, mut events) = init_with_poll();
//     let (mut s1, mut s2) = assert_ok!(UnixStream::pair());

//     assert_ok!(poll
//         .registry()
//         .register(&s1, Token(0), Interests::READABLE | Interests::WRITABLE));
//     assert_ok!(poll
//         .registry()
//         .register(&s2, Token(1), Interests::READABLE | Interests::WRITABLE));
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(Token(0), Interests::WRITABLE)],
//     );

//     let mut buf = [0; DEFAULT_BUF_SIZE];
//     assert_would_block(s1.read(&mut buf));

//     let wrote = assert_ok!(s1.write(&DATA1));
//     assert_eq!(wrote, DATA1_LEN);
//     assert_ok!(s1.flush());

//     let read = assert_ok!(s2.read(&mut buf));
//     assert_would_block(s2.read(&mut buf));
//     assert_eq!(read, DATA1_LEN);
//     assert_eq!(&buf[..read], DATA1);
//     assert_eq!(read, wrote, "unequal reads and writes");

//     let wrote = assert_ok!(s2.write(&DATA2));
//     assert_eq!(wrote, DATA2_LEN);
//     assert_ok!(s2.flush());

//     let read = assert_ok!(s1.read(&mut buf));
//     assert_eq!(read, DATA2_LEN);
//     assert_eq!(&buf[..read], DATA2);
//     assert_eq!(read, wrote, "unequal reads and writes");
// }

// #[test]
// fn uds_stream_try_clone() {
//     let (mut poll, mut events) = init_with_poll();
//     let (handle, remote_addr) = new_echo_listener(1);
//     let path = remote_addr.as_pathname().expect("not a pathname");
//     let mut stream_1 = assert_ok!(UnixStream::connect(path));

//     assert_ok!(poll
//         .registry()
//         .register(&stream_1, TOKEN_1, Interests::WRITABLE));
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
//     );

//     let mut buf = [0; DEFAULT_BUF_SIZE];
//     let wrote = assert_ok!(stream_1.write(&DATA1));
//     assert_eq!(wrote, DATA1_LEN);

//     let mut stream_2 = assert_ok!(stream_1.try_clone());

//     // When using `try_clone` the `TcpStream` needs to be deregistered!
//     assert_ok!(poll.registry().deregister(&stream_1));
//     drop(stream_1);

//     assert_ok!(poll
//         .registry()
//         .register(&stream_2, TOKEN_2, Interests::READABLE));
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_2, Interests::READABLE)],
//     );

//     let read = assert_ok!(stream_2.read(&mut buf));
//     assert_eq!(read, DATA1_LEN);
//     assert_eq!(&buf[..read], DATA1);

//     // Close the connection to allow the remote to shutdown
//     drop(stream_2);
//     assert_ok!(handle.join());
// }

// #[test]
// fn uds_stream_peer_addr() {
//     let (handle, actual_addr) = new_echo_listener(1);
//     let path = actual_addr.as_pathname().expect("not a pathname");
//     let stream = assert_ok!(UnixStream::connect(path));

//     let peer_addr = stream.peer_addr().unwrap();
//     assert_eq!(peer_addr.as_pathname().unwrap(), path);

//     // Close the connection to allow the remote to shutdown
//     drop(stream);
//     assert_ok!(handle.join());
// }

// #[test]
// fn uds_stream_register() {
//     let (mut poll, mut events) = init_with_poll();
//     let (handle, remote_addr) = new_echo_listener(1);
//     let path = remote_addr.as_pathname().expect("not a pathname");
//     let stream = assert_ok!(UnixStream::connect(path));

//     assert_ok!(poll
//         .registry()
//         .register(&stream, TOKEN_1, Interests::READABLE));
//     expect_no_events(&mut poll, &mut events);

//     // Close the connection to allow the remote to shutdown
//     drop(stream);
//     assert_ok!(handle.join());
// }

// #[test]
// fn uds_stream_reregister() {
//     let (mut poll, mut events) = init_with_poll();
//     let (handle, remote_addr) = new_echo_listener(1);
//     let path = remote_addr.as_pathname().expect("not a pathname");
//     let stream = assert_ok!(UnixStream::connect(path));

//     assert_ok!(poll
//         .registry()
//         .register(&stream, TOKEN_1, Interests::READABLE));
//     assert_ok!(poll
//         .registry()
//         .reregister(&stream, TOKEN_1, Interests::WRITABLE));
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
//     );

//     // Close the connection to allow the remote to shutdown
//     drop(stream);
//     assert_ok!(handle.join());
// }

// #[test]
// fn uds_stream_deregister() {
//     let (mut poll, mut events) = init_with_poll();
//     let (handle, remote_addr) = new_echo_listener(1);
//     let path = remote_addr.as_pathname().expect("not a pathname");
//     let stream = assert_ok!(UnixStream::connect(path));

//     assert_ok!(poll
//         .registry()
//         .register(&stream, TOKEN_1, Interests::WRITABLE));
//     assert_ok!(poll.registry().deregister(&stream));
//     expect_no_events(&mut poll, &mut events);

//     // Close the connection to allow the remote to shutdown
//     drop(stream);
//     assert_ok!(handle.join());
// }

// #[test]
// fn uds_stream_shutdown_read() {
//     let (mut poll, mut events) = init_with_poll();
//     let (handle, remote_addr) = new_echo_listener(1);
//     let path = remote_addr.as_pathname().expect("not a pathname");
//     let mut stream = assert_ok!(UnixStream::connect(path));

//     assert_ok!(poll.registry().register(
//         &stream,
//         TOKEN_1,
//         Interests::READABLE.add(Interests::WRITABLE)
//     ));
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
//     );

//     let wrote = assert_ok!(stream.write(DATA1));
//     assert_eq!(wrote, DATA1_LEN);
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
//     );

//     assert_ok!(stream.shutdown(Shutdown::Read));
//     expect_readiness!(poll, events, is_read_closed);

//     // Shutting down the reading side is different on each platform. For example
//     // on Linux based systems we can still read.
//     #[cfg(any(
//         target_os = "dragonfly",
//         target_os = "freebsd",
//         target_os = "ios",
//         target_os = "macos",
//         target_os = "netbsd",
//         target_os = "openbsd"
//     ))]
//     {
//         let mut buf = [0; DEFAULT_BUF_SIZE];
//         let read = assert_ok!(stream.read(&mut buf));
//         assert_eq!(read, 0);
//     }

//     // Close the connection to allow the remote to shutdown
//     drop(stream);
//     assert_ok!(handle.join());
// }

// #[test]
// fn uds_stream_shutdown_write() {
//     let (mut poll, mut events) = init_with_poll();
//     let (handle, remote_addr) = new_echo_listener(1);
//     let path = remote_addr.as_pathname().expect("not a pathname");
//     let mut stream = assert_ok!(UnixStream::connect(path));

//     assert_ok!(poll.registry().register(
//         &stream,
//         TOKEN_1,
//         Interests::WRITABLE.add(Interests::READABLE)
//     ));
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
//     );

//     let wrote = assert_ok!(stream.write(DATA1));
//     assert_eq!(wrote, DATA1_LEN);
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
//     );

//     assert_ok!(stream.shutdown(Shutdown::Write));

//     #[cfg(any(
//         target_os = "dragonfly",
//         target_os = "freebsd",
//         target_os = "ios",
//         target_os = "macos",
//         target_os = "netbsd",
//         target_os = "openbsd"
//     ))]
//     expect_readiness!(poll, events, is_write_closed);

//     let err = assert_err!(stream.write(DATA2));
//     assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);

//     // Read should be ok
//     let mut buf = [0; DEFAULT_BUF_SIZE];
//     let read = assert_ok!(stream.read(&mut buf));
//     assert_eq!(read, DATA1_LEN);
//     assert_eq!(&buf[..read], DATA1);

//     // Close the connection to allow the remote to shutdown
//     drop(stream);
//     assert_ok!(handle.join());
// }

// #[test]
// fn uds_stream_shutdown_both() {
//     let (mut poll, mut events) = init_with_poll();
//     let (handle, remote_addr) = new_echo_listener(1);
//     let path = remote_addr.as_pathname().expect("not a pathname");
//     let mut stream = assert_ok!(UnixStream::connect(path));

//     assert_ok!(poll.registry().register(
//         &stream,
//         TOKEN_1,
//         Interests::WRITABLE.add(Interests::READABLE)
//     ));
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
//     );

//     let wrote = assert_ok!(stream.write(DATA1));
//     assert_eq!(wrote, DATA1_LEN);
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
//     );

//     assert_ok!(stream.shutdown(Shutdown::Both));
//     expect_readiness!(poll, events, is_write_closed);

//     // Shutting down the reading side is different on each platform. For example
//     // on Linux based systems we can still read.
//     #[cfg(any(
//         target_os = "dragonfly",
//         target_os = "freebsd",
//         target_os = "ios",
//         target_os = "macos",
//         target_os = "netbsd",
//         target_os = "openbsd"
//     ))]
//     {
//         let mut buf = [0; DEFAULT_BUF_SIZE];
//         let read = assert_ok!(stream.read(&mut buf));
//         assert_eq!(read, 0);
//     }

//     let err = assert_err!(stream.write(DATA2));
//     #[cfg(unix)]
//     assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
//     #[cfg(window)]
//     assert_eq!(err.kind(), io::ErrorKind::ConnectionAbroted);

//     // Close the connection to allow the remote to shutdown
//     drop(stream);
//     assert_ok!(handle.join());
// }

// #[test]
// fn uds_stream_shutdown_listener_write() {
//     let barrier = Arc::new(Barrier::new(2));
//     let (mut poll, mut events) = init_with_poll();
//     let (handle, remote_addr) = new_noop_listener(1, barrier.clone());
//     let path = remote_addr.as_pathname().expect("not a pathname");
//     let stream = assert_ok!(UnixStream::connect(path));

//     assert_ok!(poll.registry().register(
//         &stream,
//         TOKEN_1,
//         Interests::READABLE.add(Interests::WRITABLE)
//     ));
//     expect_events(
//         &mut poll,
//         &mut events,
//         vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
//     );

//     barrier.wait();
//     expect_readiness!(poll, events, is_read_closed);

//     barrier.wait();
//     handle.join().expect("failed to join thread");
// }

fn smoke_test<F>(new_listener: F)
where
    F: FnOnce(&Path) -> io::Result<UnixListener>,
{
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));

    let dir = assert_ok!(TempDir::new("uds_listener"));
    let path = dir.path().join("smoke");
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

    let (stream, peer_addr) = assert_ok!(listener.accept());
    let stream_local = stream.local_addr().unwrap();
    let stream_peer = stream.peer_addr().unwrap();
    assert_eq!(
        stream_local
            .as_pathname()
            .expect("failed to find stream local pathname"),
        path
    );
    assert_eq!(
        stream_peer
            .as_pathname()
            .expect("failed to find stream peer pathname"),
        peer_addr.as_pathname().expect("failed!!!!!!!!!")
    );

    barrier.wait();
    assert_ok!(handle.join());
}

/// Open `n_connections` connections to `sock_addr`. If a `barrier` is
/// provided it will wait on it after each connection is made before it is
/// dropped.
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

// fn new_echo_listener(connections: usize) -> (thread::JoinHandle<()>, net::SocketAddr) {
//     let (addr_sender, addr_receiver) = channel();
//     let handle = thread::spawn(move || {
//         let dir = assert_ok!(TempDir::new("uds"));
//         let path = dir.path().join("foo");
//         let listener = assert_ok!(net::UnixListener::bind(path));
//         let sock_addr = assert_ok!(listener.local_addr());
//         assert_ok!(addr_sender.send(sock_addr));

//         for _ in 0..connections {
//             let (mut stream, _) = assert_ok!(listener.accept());

//             // On Linux based system it will cause a connection reset
//             // error when the reading side of the peer connection is
//             // shutdown, we don't consider it an actual here.
//             let (mut read, mut written) = (0, 0);
//             let mut buf = [0; DEFAULT_BUF_SIZE];
//             loop {
//                 let n = match stream.try_read(&mut buf) {
//                     Ok(Some(amount)) => {
//                         read += amount;
//                         amount
//                     }
//                     Ok(None) => continue,
//                     Err(ref err) if err.kind() == io::ErrorKind::ConnectionReset => break,
//                     Err(err) => panic!("{}", err),
//                 };
//                 if n == 0 {
//                     break;
//                 }
//                 match stream.try_write(&buf[..n]) {
//                     Ok(Some(amount)) => written += amount,
//                     Ok(None) => continue,
//                     Err(ref err) if err.kind() == io::ErrorKind::BrokenPipe => break,
//                     Err(err) => panic!("{:?}", err),
//                 };
//             }
//             assert_eq!(read, written, "unequal reads and writes");
//         }
//     });
//     (handle, assert_ok!(addr_receiver.recv()))
// }

// fn new_noop_listener(
//     connections: usize,
//     barrier: Arc<Barrier>,
// ) -> (thread::JoinHandle<()>, net::SocketAddr) {
//     let (sender, receiver) = channel();
//     let handle = thread::spawn(move || {
//         let dir = assert_ok!(TempDir::new("uds"));
//         let path = dir.path().join("foo");
//         let listener = assert_ok!(net::UnixListener::bind(path));
//         let sock_addr = assert_ok!(listener.local_addr());
//         assert_ok!(sender.send(sock_addr));

//         for _ in 0..connections {
//             let (stream, _) = listener.accept().unwrap();
//             barrier.wait();
//             assert_ok!(stream.shutdown(Shutdown::Write));
//             barrier.wait();
//             drop(stream);
//         }
//     });
//     (handle, assert_ok!(receiver.recv()))
// }
