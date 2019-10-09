#![cfg(unix)]
#[macro_use]
mod util;

use mio::net::{UnixDatagram, UnixListener, UnixStream};
use mio::unix::SocketAddr;
use mio::{Interests, Token};
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::Shutdown;
use std::sync::mpsc::{channel, Receiver};
use std::thread;
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

const LOCAL: Token = Token(0);
const LOCAL_CLONE: Token = Token(1);

#[test]
fn smoke_test() {
    let (mut poll, mut events) = init_with_poll();

    let (sync_sender, sync_receiver) = channel();
    let (handle, remote_addr) = echo_remote(1, sync_receiver);

    let path = remote_addr.as_pathname().expect("not a pathname");
    let mut local = assert_ok!(UnixStream::connect(path));
    assert_ok!(sync_sender.send(()));

    assert_ok!(poll.registry().register(
        &local,
        LOCAL,
        Interests::WRITABLE.add(Interests::READABLE)
    ));

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::WRITABLE)],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(local.read(&mut buf));

    let wrote = assert_ok!(local.write(&DATA1));
    assert_eq!(wrote, DATA1_LEN);
    assert_ok!(local.flush());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
    );

    let read = assert_ok!(local.read(&mut buf));
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);
    assert_eq!(read, wrote, "unequal reads and writes");

    assert!(assert_ok!(local.take_error()).is_none());

    let bufs = [IoSlice::new(&DATA1), IoSlice::new(&DATA2)];
    let wrote = assert_ok!(local.write_vectored(&bufs));
    assert_eq!(wrote, DATA1_LEN + DATA2_LEN);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
    );

    let mut buf1 = [1; DATA1_LEN];
    let mut buf2 = [2; DATA2_LEN + 1];
    let mut bufs = [IoSliceMut::new(&mut buf1), IoSliceMut::new(&mut buf2)];
    let read = assert_ok!(local.read_vectored(&mut bufs));
    assert_eq!(read, DATA1_LEN + DATA2_LEN);
    assert_eq!(&buf1, DATA1);
    assert_eq!(&buf2[..DATA2.len()], DATA2);

    // Last byte should be unchanged
    assert_eq!(buf2[DATA2.len()], 2);

    // Close the connection to allow the remote to shutdown
    drop(local);
    assert_ok!(handle.join());
}

#[test]
fn is_send_and_sync() {
    assert_send::<UnixListener>();
    assert_sync::<UnixListener>();

    assert_send::<UnixStream>();
    assert_sync::<UnixStream>();

    assert_send::<UnixDatagram>();
    assert_sync::<UnixDatagram>();
}

#[test]
fn register() {
    let (mut poll, mut events) = init_with_poll();

    let (sync_sender, sync_receiver) = channel();
    let (handle, remote_addr) = echo_remote(1, sync_receiver);

    let path = remote_addr.as_pathname().expect("not a pathname");
    let local = assert_ok!(UnixStream::connect(path));
    assert_ok!(sync_sender.send(()));

    assert_ok!(poll.registry().register(&local, LOCAL, Interests::READABLE));

    expect_no_events(&mut poll, &mut events);

    // Close the connection to allow the remote to shutdown
    drop(local);
    assert_ok!(handle.join());
}

#[test]
fn reregister() {
    let (mut poll, mut events) = init_with_poll();

    let (sync_sender, sync_receiver) = channel();
    let (handle, remote_addr) = echo_remote(1, sync_receiver);

    let path = remote_addr.as_pathname().expect("not a pathname");
    let local = assert_ok!(UnixStream::connect(path));
    assert_ok!(sync_sender.send(()));

    assert_ok!(poll.registry().register(&local, LOCAL, Interests::READABLE));
    assert_ok!(poll
        .registry()
        .reregister(&local, LOCAL_CLONE, Interests::WRITABLE));

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL_CLONE, Interests::WRITABLE)],
    );

    // Close the connection to allow the remote to shutdown
    drop(local);
    assert_ok!(handle.join());
}

#[test]
fn deregister() {
    let (mut poll, mut events) = init_with_poll();

    let (sync_sender, sync_receiver) = channel();
    let (handle, remote_addr) = echo_remote(1, sync_receiver);

    let path = remote_addr.as_pathname().expect("not a pathname");
    let local = assert_ok!(UnixStream::connect(path));
    assert_ok!(sync_sender.send(()));

    assert_ok!(poll.registry().register(&local, LOCAL, Interests::WRITABLE));
    assert_ok!(poll.registry().deregister(&local));

    expect_no_events(&mut poll, &mut events);

    // Close the connection to allow the remote to shutdown
    drop(local);
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
fn try_clone() {
    let (mut poll, mut events) = init_with_poll();

    let (sync_sender, sync_receiver) = channel();
    let (handle, remote_addr) = echo_remote(1, sync_receiver);

    let path = remote_addr.as_pathname().expect("not a pathname");
    let mut local_1 = assert_ok!(UnixStream::connect(path));
    assert_ok!(sync_sender.send(()));

    assert_ok!(poll
        .registry()
        .register(&local_1, LOCAL, Interests::WRITABLE));

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::WRITABLE)],
    );

    let mut buf = [0; DEFAULT_BUF_SIZE];
    let wrote = assert_ok!(local_1.write(&DATA1));
    assert_eq!(wrote, DATA1_LEN);

    let mut local_2 = assert_ok!(local_1.try_clone());

    // When using `try_clone` the `TcpStream` needs to be deregistered!
    assert_ok!(poll.registry().deregister(&local_1));
    drop(local_1);

    assert_ok!(poll
        .registry()
        .register(&local_2, LOCAL_CLONE, Interests::READABLE));

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL_CLONE, Interests::READABLE)],
    );

    let read = assert_ok!(local_2.read(&mut buf));
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);

    // Close the connection to allow the remote to shutdown
    drop(local_2);
    assert_ok!(handle.join());
}

#[test]
fn shutdown_read() {
    let (mut poll, mut events) = init_with_poll();

    let (sync_sender, sync_receiver) = channel();
    let (handle, remote_addr) = echo_remote(1, sync_receiver);

    let path = remote_addr.as_pathname().expect("not a pathname");
    let mut local = assert_ok!(UnixStream::connect(path));
    assert_ok!(sync_sender.send(()));

    assert_ok!(poll.registry().register(
        &local,
        LOCAL,
        Interests::WRITABLE.add(Interests::READABLE)
    ));

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::WRITABLE)],
    );

    let wrote = assert_ok!(local.write(DATA1));
    assert_eq!(wrote, DATA1_LEN);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
    );

    assert_ok!(local.shutdown(Shutdown::Read));

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
        let read = assert_ok!(local.read(&mut buf));
        assert_eq!(read, 0);
    }

    // Close the connection to allow the remote to shutdown
    drop(local);
    assert_ok!(handle.join());
}

#[test]
fn shutdown_write() {
    let (mut poll, mut events) = init_with_poll();

    let (sync_sender, sync_receiver) = channel();
    let (handle, remote_addr) = echo_remote(1, sync_receiver);

    let path = remote_addr.as_pathname().expect("not a pathname");
    let mut local = assert_ok!(UnixStream::connect(path));
    assert_ok!(sync_sender.send(()));

    assert_ok!(poll.registry().register(
        &local,
        LOCAL,
        Interests::WRITABLE.add(Interests::READABLE)
    ));

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::WRITABLE)],
    );

    let wrote = assert_ok!(local.write(DATA1));
    assert_eq!(wrote, DATA1_LEN);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
    );

    assert_ok!(local.shutdown(Shutdown::Write));

    let err = assert_err!(local.write(DATA2));
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
    );

    // Read should be ok
    let mut buf = [0; DEFAULT_BUF_SIZE];
    let read = assert_ok!(local.read(&mut buf));
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);

    // Close the connection to allow the remote to shutdown
    drop(local);
    assert_ok!(handle.join());
}

#[test]
fn shutdown_both() {
    let (mut poll, mut events) = init_with_poll();

    let (sync_sender, sync_receiver) = channel();
    let (handle, remote_addr) = echo_remote(1, sync_receiver);

    let path = remote_addr.as_pathname().expect("not a pathname");
    let mut local = assert_ok!(UnixStream::connect(path));
    assert_ok!(sync_sender.send(()));

    assert_ok!(poll.registry().register(
        &local,
        LOCAL,
        Interests::WRITABLE.add(Interests::READABLE)
    ));

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::WRITABLE)],
    );

    let wrote = assert_ok!(local.write(DATA1));
    assert_eq!(wrote, DATA1_LEN);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
    );

    assert_ok!(local.shutdown(Shutdown::Both));

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
        let read = assert_ok!(local.read(&mut buf));
        assert_eq!(read, 0);
    }

    let err = assert_err!(local.write(DATA2));
    #[cfg(unix)]
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    #[cfg(window)]
    assert_eq!(err.kind(), io::ErrorKind::ConnectionAbroted);

    // Close the connection to allow the remote to shutdown
    drop(local);
    assert_ok!(handle.join());
}

fn echo_remote(
    connections: usize,
    sync_receiver: Receiver<()>,
) -> (thread::JoinHandle<()>, SocketAddr) {
    let (addr_sender, addr_receiver) = channel();
    let handle = thread::spawn(move || {
        let dir = assert_ok!(TempDir::new("uds"));
        let path = dir.path().join("foo");
        let remote = assert_ok!(UnixListener::bind(path.clone()));
        let local_address = assert_ok!(remote.local_addr());
        assert_ok!(addr_sender.send(local_address));

        for _ in 0..connections {
            assert_ok!(sync_receiver.recv());
            let (mut local, _) = assert_ok!(remote.accept());

            // On Linux based system it will cause a connection reset
            // error when the reading side of the peer connection is
            // shutdown, we don't consider it an actual here.
            let (mut read, mut written) = (0, 0);
            let mut buf = [0; DEFAULT_BUF_SIZE];
            loop {
                let n = match local.try_read(&mut buf) {
                    Ok(Some(amount)) => {
                        read += amount;
                        amount
                    }
                    Ok(None) => continue,
                    Err(ref err) if err.kind() == io::ErrorKind::ConnectionReset => break,
                    Err(err) => panic!("{}", err),
                };
                match local.try_write(&buf[..n]) {
                    Ok(Some(amount)) => written += amount,
                    Ok(None) => continue,
                    Err(ref err) if err.kind() == io::ErrorKind::BrokenPipe => break,
                    Err(err) => panic!("{:?}", err),
                };
            }
            assert_eq!(read, written, "unequal reads and writes");
        }
    });
    (handle, assert_ok!(addr_receiver.recv()))
}
