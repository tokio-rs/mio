#![cfg(unix)]

mod util;

use mio::net::UnixStream;
use mio::{Interests, Token};
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::Shutdown;
use std::os::unix::net;
use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::{Arc, Barrier};
use std::thread;
use tempdir::TempDir;
use util::{
    assert_send, assert_sync, assert_would_block, expect_events, expect_no_events, init_with_poll,
    ExpectEvent, Readiness,
};

const DATA1: &[u8] = b"Hello same host!";
const DATA2: &[u8] = b"Why hello mio!";
const DATA1_LEN: usize = 16;
const DATA2_LEN: usize = 14;
const DEFAULT_BUF_SIZE: usize = 64;
const TOKEN_1: Token = Token(0);
const TOKEN_2: Token = Token(1);

#[test]
fn unix_stream_send_and_sync() {
    assert_send::<UnixStream>();
    assert_sync::<UnixStream>();
}

#[test]
fn unix_stream_smoke() {
    #[allow(clippy::redundant_closure)]
    smoke_test(|path| UnixStream::connect(path));
}

#[test]
fn unix_stream_connect() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let dir = TempDir::new("unix").unwrap();
    let path = dir.path().join("any");

    let listener = net::UnixListener::bind(path.clone()).unwrap();
    let stream = UnixStream::connect(path).unwrap();

    let barrier_clone = barrier.clone();
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        barrier_clone.wait();
        drop(stream);
    });

    poll.registry()
        .register(&stream, TOKEN_1, Interests::READABLE | Interests::WRITABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    barrier.wait();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );

    handle.join().unwrap();
}

#[test]
fn unix_stream_from_std() {
    smoke_test(|path| {
        let local = net::UnixStream::connect(path).unwrap();
        // `std::os::unix::net::UnixStream`s are blocking by default, so make sure
        // it is in non-blocking mode before wrapping in a Mio equivalent.
        local.set_nonblocking(true).unwrap();
        Ok(UnixStream::from_std(local))
    })
}

#[test]
fn unix_stream_pair() {
    let (mut poll, mut events) = init_with_poll();

    let (mut s1, mut s2) = UnixStream::pair().unwrap();
    poll.registry()
        .register(&s1, TOKEN_1, Interests::READABLE | Interests::WRITABLE)
        .unwrap();
    poll.registry()
        .register(&s2, TOKEN_2, Interests::READABLE | Interests::WRITABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(s1.read(&mut buf));

    let wrote = s1.write(&DATA1).unwrap();
    assert_eq!(wrote, DATA1_LEN);
    s1.flush().unwrap();

    let read = s2.read(&mut buf).unwrap();
    assert_would_block(s2.read(&mut buf));
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);
    assert_eq!(read, wrote, "unequal reads and writes");

    let wrote = s2.write(&DATA2).unwrap();
    assert_eq!(wrote, DATA2_LEN);
    s2.flush().unwrap();

    let read = s1.read(&mut buf).unwrap();
    assert_eq!(read, DATA2_LEN);
    assert_eq!(&buf[..read], DATA2);
    assert_eq!(read, wrote, "unequal reads and writes");
}

#[test]
fn unix_stream_try_clone() {
    let (mut poll, mut events) = init_with_poll();
    let (handle, remote_addr) = new_echo_listener(1);
    let path = remote_addr.as_pathname().expect("failed to get pathname");

    let mut stream_1 = UnixStream::connect(path).unwrap();
    poll.registry()
        .register(&stream_1, TOKEN_1, Interests::WRITABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    let wrote = stream_1.write(&DATA1).unwrap();
    assert_eq!(wrote, DATA1_LEN);

    let mut stream_2 = stream_1.try_clone().unwrap();

    // When using `try_clone` the `TcpStream` needs to be deregistered!
    poll.registry().deregister(&stream_1).unwrap();
    drop(stream_1);

    poll.registry()
        .register(&stream_2, TOKEN_2, Interests::READABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_2, Interests::READABLE)],
    );

    let read = stream_2.read(&mut buf).unwrap();
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);

    // Close the connection to allow the remote to shutdown
    drop(stream_2);
    handle.join().unwrap();
}

#[test]
fn unix_stream_peer_addr() {
    let (handle, expected_addr) = new_echo_listener(1);
    let expected_path = expected_addr.as_pathname().expect("failed to get pathname");

    let stream = UnixStream::connect(expected_path).unwrap();

    assert_eq!(
        stream.peer_addr().unwrap().as_pathname().unwrap(),
        expected_path
    );
    assert!(stream.local_addr().unwrap().as_pathname().is_none());

    // Close the connection to allow the remote to shutdown
    drop(stream);
    handle.join().unwrap();
}

#[test]
fn unix_stream_shutdown_read() {
    let (mut poll, mut events) = init_with_poll();
    let (handle, remote_addr) = new_echo_listener(1);
    let path = remote_addr.as_pathname().expect("failed to get pathname");

    let mut stream = UnixStream::connect(path).unwrap();
    poll.registry()
        .register(
            &stream,
            TOKEN_1,
            Interests::READABLE.add(Interests::WRITABLE),
        )
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    let wrote = stream.write(DATA1).unwrap();
    assert_eq!(wrote, DATA1_LEN);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );

    stream.shutdown(Shutdown::Read).unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Readiness::READ_CLOSED)],
    );

    // Shutting down the reading side is different on each platform. For example
    // on Linux based systems we can still read.
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        let mut buf = [0; DEFAULT_BUF_SIZE];
        let read = stream.read(&mut buf).unwrap();
        assert_eq!(read, 0);
    }

    // Close the connection to allow the remote to shutdown
    drop(stream);
    handle.join().unwrap();
}

#[test]
fn unix_stream_shutdown_write() {
    let (mut poll, mut events) = init_with_poll();
    let (handle, remote_addr) = new_echo_listener(1);
    let path = remote_addr.as_pathname().expect("failed to get pathname");

    let mut stream = UnixStream::connect(path).unwrap();
    poll.registry()
        .register(
            &stream,
            TOKEN_1,
            Interests::WRITABLE.add(Interests::READABLE),
        )
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    let wrote = stream.write(DATA1).unwrap();
    assert_eq!(wrote, DATA1_LEN);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );

    stream.shutdown(Shutdown::Write).unwrap();

    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Readiness::WRITE_CLOSED)],
    );

    let err = stream.write(DATA2).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);

    // Read should be ok
    let mut buf = [0; DEFAULT_BUF_SIZE];
    let read = stream.read(&mut buf).unwrap();
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);

    // Close the connection to allow the remote to shutdown
    drop(stream);
    handle.join().unwrap();
}

#[test]
fn unix_stream_shutdown_both() {
    let (mut poll, mut events) = init_with_poll();
    let (handle, remote_addr) = new_echo_listener(1);
    let path = remote_addr.as_pathname().expect("failed to get pathname");

    let mut stream = UnixStream::connect(path).unwrap();
    poll.registry()
        .register(
            &stream,
            TOKEN_1,
            Interests::WRITABLE.add(Interests::READABLE),
        )
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    let wrote = stream.write(DATA1).unwrap();
    assert_eq!(wrote, DATA1_LEN);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );

    stream.shutdown(Shutdown::Both).unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Readiness::WRITE_CLOSED)],
    );

    // Shutting down the reading side is different on each platform. For example
    // on Linux based systems we can still read.
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        let mut buf = [0; DEFAULT_BUF_SIZE];
        let read = stream.read(&mut buf).unwrap();
        assert_eq!(read, 0);
    }

    let err = stream.write(DATA2).unwrap_err();
    #[cfg(unix)]
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    #[cfg(window)]
    assert_eq!(err.kind(), io::ErrorKind::ConnectionAbroted);

    // Close the connection to allow the remote to shutdown
    drop(stream);
    handle.join().unwrap();
}

#[test]
fn unix_stream_shutdown_listener_write() {
    let barrier = Arc::new(Barrier::new(2));
    let (mut poll, mut events) = init_with_poll();
    let (handle, remote_addr) = new_noop_listener(1, barrier.clone());
    let path = remote_addr.as_pathname().expect("failed to get pathname");

    let stream = UnixStream::connect(path).unwrap();
    poll.registry()
        .register(
            &stream,
            TOKEN_1,
            Interests::READABLE.add(Interests::WRITABLE),
        )
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    barrier.wait();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Readiness::READ_CLOSED)],
    );

    barrier.wait();
    handle.join().unwrap();
}

#[test]
fn unix_stream_register() {
    let (mut poll, mut events) = init_with_poll();
    let (handle, remote_addr) = new_echo_listener(1);
    let path = remote_addr.as_pathname().expect("failed to get pathname");

    let stream = UnixStream::connect(path).unwrap();
    poll.registry()
        .register(&stream, TOKEN_1, Interests::READABLE)
        .unwrap();
    expect_no_events(&mut poll, &mut events);

    // Close the connection to allow the remote to shutdown
    drop(stream);
    handle.join().unwrap();
}

#[test]
fn unix_stream_reregister() {
    let (mut poll, mut events) = init_with_poll();
    let (handle, remote_addr) = new_echo_listener(1);
    let path = remote_addr.as_pathname().expect("failed to get pathname");

    let stream = UnixStream::connect(path).unwrap();
    poll.registry()
        .register(&stream, TOKEN_1, Interests::READABLE)
        .unwrap();
    poll.registry()
        .reregister(&stream, TOKEN_1, Interests::WRITABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    // Close the connection to allow the remote to shutdown
    drop(stream);
    handle.join().unwrap();
}

#[test]
fn unix_stream_deregister() {
    let (mut poll, mut events) = init_with_poll();
    let (handle, remote_addr) = new_echo_listener(1);
    let path = remote_addr.as_pathname().expect("failed to get pathname");

    let stream = UnixStream::connect(path).unwrap();
    poll.registry()
        .register(&stream, TOKEN_1, Interests::WRITABLE)
        .unwrap();
    poll.registry().deregister(&stream).unwrap();
    expect_no_events(&mut poll, &mut events);

    // Close the connection to allow the remote to shutdown
    drop(stream);
    handle.join().unwrap();
}

fn smoke_test<F>(connect_stream: F)
where
    F: FnOnce(&Path) -> io::Result<UnixStream>,
{
    let (mut poll, mut events) = init_with_poll();
    let (handle, remote_addr) = new_echo_listener(1);
    let path = remote_addr.as_pathname().expect("failed to get pathname");

    let mut stream = connect_stream(path).unwrap();
    poll.registry()
        .register(
            &stream,
            TOKEN_1,
            Interests::WRITABLE.add(Interests::READABLE),
        )
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::WRITABLE)],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(stream.read(&mut buf));

    let wrote = stream.write(&DATA1).unwrap();
    assert_eq!(wrote, DATA1_LEN);
    stream.flush().unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );

    let read = stream.read(&mut buf).unwrap();
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);
    assert_eq!(read, wrote, "unequal reads and writes");

    assert!(stream.take_error().unwrap().is_none());

    let bufs = [IoSlice::new(&DATA1), IoSlice::new(&DATA2)];
    let wrote = stream.write_vectored(&bufs).unwrap();
    assert_eq!(wrote, DATA1_LEN + DATA2_LEN);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interests::READABLE)],
    );

    let mut buf1 = [1; DATA1_LEN];
    let mut buf2 = [2; DATA2_LEN + 1];
    let mut bufs = [IoSliceMut::new(&mut buf1), IoSliceMut::new(&mut buf2)];
    let read = stream.read_vectored(&mut bufs).unwrap();
    assert_eq!(read, DATA1_LEN + DATA2_LEN);
    assert_eq!(&buf1, DATA1);
    assert_eq!(&buf2[..DATA2.len()], DATA2);

    // Last byte should be unchanged
    assert_eq!(buf2[DATA2.len()], 2);

    // Close the connection to allow the remote to shutdown
    drop(stream);
    handle.join().unwrap();
}

fn new_echo_listener(connections: usize) -> (thread::JoinHandle<()>, net::SocketAddr) {
    let (addr_sender, addr_receiver) = channel();
    let handle = thread::spawn(move || {
        let dir = TempDir::new("unix").unwrap();
        let path = dir.path().join("any");
        let listener = net::UnixListener::bind(path).unwrap();
        let local_addr = listener.local_addr().unwrap();
        addr_sender.send(local_addr).unwrap();

        for _ in 0..connections {
            let (mut stream, _) = listener.accept().unwrap();

            // On Linux based system it will cause a connection reset
            // error when the reading side of the peer connection is
            // shutdown, we don't consider it an actual here.
            let (mut read, mut written) = (0, 0);
            let mut buf = [0; DEFAULT_BUF_SIZE];
            loop {
                let n = match stream.read(&mut buf) {
                    Ok(amount) => {
                        read += amount;
                        amount
                    }
                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(ref err) if err.kind() == io::ErrorKind::ConnectionReset => break,
                    Err(err) => panic!("{}", err),
                };
                if n == 0 {
                    break;
                }
                match stream.write(&buf[..n]) {
                    Ok(amount) => written += amount,
                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(ref err) if err.kind() == io::ErrorKind::BrokenPipe => break,
                    Err(err) => panic!("{}", err),
                };
            }
            assert_eq!(read, written, "unequal reads and writes");
        }
    });
    (handle, addr_receiver.recv().unwrap())
}

fn new_noop_listener(
    connections: usize,
    barrier: Arc<Barrier>,
) -> (thread::JoinHandle<()>, net::SocketAddr) {
    let (sender, receiver) = channel();
    let handle = thread::spawn(move || {
        let dir = TempDir::new("unix").unwrap();
        let path = dir.path().join("any");
        let listener = net::UnixListener::bind(path).unwrap();
        let local_addr = listener.local_addr().unwrap();
        sender.send(local_addr).unwrap();

        for _ in 0..connections {
            let (stream, _) = listener.accept().unwrap();
            barrier.wait();
            stream.shutdown(Shutdown::Write).unwrap();
            barrier.wait();
            drop(stream);
        }
    });
    (handle, receiver.recv().unwrap())
}
