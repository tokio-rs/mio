use std::io;
use std::time::Duration;
use std::{net, thread};

use mio::net::TcpListener;
use mio::{Events, Interests, Poll, Token};

use crate::util::{assert_send, assert_sync, localhost};

const LISTENER: Token = Token(0);

#[test]
fn is_send_and_sync() {
    assert_send::<TcpListener>();
    assert_sync::<TcpListener>();
}

#[test]
fn accepting_streams() {
    struct H {
        hit: bool,
        listener: TcpListener,
        shutdown: bool,
    }

    let l = TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        net::TcpStream::connect(addr).unwrap();
    });

    let mut poll = Poll::new().unwrap();

    poll.registry()
        .register(&l, LISTENER, Interests::READABLE)
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
            assert_eq!(event.token(), LISTENER);
            assert!(event.is_readable());
            assert!(h.listener.accept().is_ok());
            h.shutdown = true;
        }
    }
    assert!(h.hit);
    assert!(h.listener.accept().unwrap_err().kind() == io::ErrorKind::WouldBlock);
    t.join().unwrap();
}

#[test]
fn register_and_drop() {
    let mut poll = Poll::new().unwrap();
    let l = TcpListener::bind(localhost()).unwrap();

    poll.registry()
        .register(&l, LISTENER, Interests::READABLE)
        .unwrap();
    drop(l);

    let mut events = Events::with_capacity(128);

    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
    assert!(events.is_empty());
}

#[test]
fn no_port_reuse() {
    let l1 = TcpListener::bind(localhost()).unwrap();
    let addr = l1.local_addr().unwrap();
    assert!(TcpListener::bind(addr).is_err());
}
