mod util;

use mio::net::{UnixListener, UnixStream};
use mio::{Events, Interests, Poll, Token};
use std::io;
#[cfg(unix)]
use std::os::unix::net;
use std::thread;
use tempdir::TempDir;
use util::{assert_send, assert_sync, init};

const LISTEN: Token = Token(0);
// const CLIENT: Token = Token(1);
// const SERVER: Token = Token(2);

#[test]
fn is_send_and_sync() {
    assert_send::<UnixListener>();
    assert_sync::<UnixListener>();

    assert_send::<UnixStream>();
    assert_sync::<UnixStream>();
}

#[test]
fn accept() {
    init();

    struct H {
        hit: bool,
        listener: UnixListener,
        shutdown: bool,
    }

    let temp_dir = TempDir::new("uds").unwrap();
    let addr = temp_dir.path().join("sock");

    let l = UnixListener::bind(addr.clone()).unwrap();

    let t = thread::spawn(move || {
        net::UnixStream::connect(addr).unwrap();
    });

    let mut poll = Poll::new().unwrap();

    poll.registry()
        .register(&l, LISTEN, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        hit: false,
        listener: l,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            h.hit = true;
            assert_eq!(event.token(), LISTEN);
            assert!(event.is_readable());
            assert!(h.listener.accept().is_ok());
            h.shutdown = true;
        }
    }
    assert!(h.hit);
    assert!(h.listener.accept().unwrap_err().kind() == io::ErrorKind::WouldBlock);
    t.join().unwrap();
}
