#![cfg(all(feature = "os-poll", feature = "tcp"))]

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token, Waker};
use std::io::{self, Read};
use std::sync::Arc;
use std::time::Duration;
use std::{net, thread};

mod util;
use util::{any_local_address, init, init_with_poll};

const ID1: Token = Token(1);
const WAKE_TOKEN: Token = Token(10);

#[test]
fn issue_776() {
    init();

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().expect("accept").0;
        s.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set_read_timeout");
        let _ = s.read(&mut [0; 16]).expect("read");
    });

    let mut poll = Poll::new().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&mut s, Token(1), Interest::READABLE | Interest::WRITABLE)
        .unwrap();
    let mut events = Events::with_capacity(16);
    'outer: loop {
        poll.poll(&mut events, None).unwrap();
        for event in &events {
            if event.token() == Token(1) {
                // connected
                break 'outer;
            }
        }
    }

    let mut b = [0; 1024];
    match s.read(&mut b) {
        Ok(_) => panic!("unexpected ok"),
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => (),
        Err(e) => panic!("unexpected error: {:?}", e),
    }

    drop(s);
    t.join().unwrap();
}

#[test]
fn issue_1205() {
    let (mut poll, mut events) = init_with_poll();

    let waker = Arc::new(Waker::new(poll.registry(), WAKE_TOKEN).unwrap());

    // `_waker` must stay in scope in order for `Waker` events to be delivered
    // when the test polls for events. If it is not cloned, it is moved out of
    // scope in `thread::spawn` and `Poll::poll` will timeout.
    #[allow(clippy::redundant_clone)]
    let _waker = waker.clone();

    let mut listener = TcpListener::bind(any_local_address()).unwrap();

    poll.registry()
        .register(&mut listener, ID1, Interest::READABLE)
        .unwrap();

    poll.poll(&mut events, Some(std::time::Duration::from_millis(0)))
        .unwrap();
    assert!(events.iter().count() == 0);

    let _stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();

    poll.registry().deregister(&mut listener).unwrap();

    // spawn a waker thread to wake the poll call below
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        waker.wake().expect("unable to wake");
    });

    poll.poll(&mut events, None).unwrap();

    // the poll should return only one event that being the waker event.
    // the poll should not retrieve event for the listener above because it was
    // deregistered
    assert!(events.iter().count() == 1);
    let waker_event = events.iter().next().unwrap();
    assert!(waker_event.is_readable());
    assert_eq!(waker_event.token(), WAKE_TOKEN);
    handle.join().unwrap();
}
