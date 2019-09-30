#[macro_use]
mod util;

use mio::net::{UnixListener, UnixStream};
use mio::{Interests, Token};
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
#[cfg(unix)]
use std::os::unix::net::SocketAddr;
use std::sync::mpsc::{channel, Receiver};
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
const LOCAL_CLONE: Token = Token(2);

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
fn stream() {
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

    let mut buf = [0; 64];
    assert_would_block(local.read(&mut buf));

    let (mut read, mut written) = (0, 0);
    while written < DATA1_LEN {
        if let Some(amount) = assert_ok!(local.try_write(&DATA1)) {
            written += amount;
        }
    }
    assert_eq!(written, DATA1_LEN);
    assert_ok!(local.flush());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(LOCAL, Interests::READABLE)],
    );

    while read < DATA1_LEN {
        if let Some(amount) = assert_ok!(local.try_read(&mut buf)) {
            read += amount;
        }
    }
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);
    assert_eq!(read, written, "unequal reads and writes");

    assert!(assert_ok!(local.take_error()).is_none());

    let bufs = [IoSlice::new(&DATA1), IoSlice::new(&DATA2)];
    let written = assert_ok!(local.write_vectored(&bufs));
    assert_eq!(written, DATA1_LEN + DATA2_LEN);

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
    assert_eq!(buf2[DATA2.len()], 2); // Last byte should be unchanged.

    // Close the connection to allow the remote to shutdown
    drop(local);
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

    let mut buf = [0; 64];
    let mut written = 0;
    while written < DATA1_LEN {
        if let Some(amount) = assert_ok!(local_1.try_write(&DATA1)) {
            written += amount;
        }
    }
    assert_eq!(written, DATA1_LEN);

    let mut local_2 = local_1.try_clone().unwrap();

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

    let mut read = 0;
    while read < DATA1_LEN {
        if let Some(amount) = assert_ok!(local_2.try_read(&mut buf)) {
            read += amount;
        }
    }
    assert_eq!(read, DATA1_LEN);
    assert_eq!(&buf[..read], DATA1);

    // Close the connection to allow the remote to shutdown
    drop(local_2);
    handle.join().expect("unable to join thread");
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
            let mut buf = [0; 64];
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
