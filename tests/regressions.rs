use std::io::{self, Read};

use std::time::{Duration, Instant};

use std::sync::{Arc, Barrier};
use std::{net, thread};

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token, Waker};

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
    let waker1 = waker.clone();

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
        waker1.wake().expect("unable to wake");
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

#[test]
fn issue_1189() {
    init();

    for _idx in 0..1000 {
        let mut poll = Poll::new().unwrap();
        let registry1 = Arc::new(poll.registry().try_clone().unwrap());
        let registry2 = Arc::clone(&registry1);
        let registry3 = Arc::clone(&registry1);

        let join_barrier1 = Arc::new(Barrier::new(3));
        let join_barrier2 = Arc::clone(&join_barrier1);
        let join_barrier3 = Arc::clone(&join_barrier1);

        // Used to make sure the listener is ready to accept connections before
        // the stream attempts connect. If there's no one listening the connect
        // packet is dropped and it is retransmitted ~500ms later because
        // there's been no response.
        let listen_barrier2 = Arc::new(Barrier::new(2));
        let listen_barrier3 = Arc::clone(&listen_barrier2);

        let mut events = Events::with_capacity(128);
        let addr = "127.0.0.1:9999".parse().unwrap();

        let thread_handle2 = thread::spawn(move || {
            let mut listener2 = TcpListener::bind(addr).unwrap();
            listen_barrier2.wait();

            registry2
                .register(&mut listener2, Token(0), Interest::READABLE)
                .unwrap();

            join_barrier2.wait();
        });

        let thread_handle3 = thread::spawn(move || {
            listen_barrier3.wait();
            let mut stream3 = TcpStream::connect(addr).unwrap();
            registry3
                .register(
                    &mut stream3,
                    Token(1),
                    Interest::READABLE | Interest::WRITABLE,
                )
                .unwrap();

            join_barrier3.wait();
        });

        let now = Instant::now();
        poll.poll(&mut events, None).unwrap();
        assert!(now.elapsed().as_millis() <= 10);
        assert!(events.iter().count() >= 1);

        // Let the threads return.
        join_barrier1.wait();

        thread_handle2.join().unwrap();
        thread_handle3.join().unwrap();
    }
}
