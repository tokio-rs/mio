use mio::net::{TcpListener, TcpStream};
use mio::*;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

mod util;

use util::{init, localhost};

#[test]
fn test_poll_closes_fd() {
    init();

    for _ in 0..2000 {
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(4);

        poll.poll(&mut events, Some(Duration::from_millis(0)))
            .unwrap();

        drop(poll);
    }
}

#[test]
fn test_drop_cancels_interest_and_shuts_down() {
    init();

    use mio::net::TcpStream;
    use std::io;
    use std::io::Read;
    use std::net::TcpListener;
    use std::thread;

    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.incoming().next().unwrap().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set_read_timeout");
        let r = s.read(&mut [0; 16]);
        match r {
            Ok(_) => (),
            Err(e) => {
                if e.kind() != io::ErrorKind::UnexpectedEof {
                    panic!(e);
                }
            }
        }
    });

    let mut poll = Poll::new().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, Token(1), Interests::READABLE | Interests::WRITABLE)
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
fn test_registry_behind_arc() {
    // `Registry` should work behind an `Arc`, being `Sync` and `Send`.
    init();

    let mut poll = Poll::new().unwrap();
    let registry = Arc::new(poll.registry().try_clone().unwrap());
    let mut events = Events::with_capacity(128);

    let addr = localhost();
    let barrier = Arc::new(Barrier::new(3));

    let registry2 = Arc::clone(&registry);
    let registry3 = Arc::clone(&registry);
    let barrier2 = Arc::clone(&barrier);
    let barrier3 = Arc::clone(&barrier);

    let handle1 = thread::spawn(move || {
        let listener = TcpListener::bind(addr).unwrap();
        registry2
            .register(&listener, Token(0), Interests::READABLE)
            .unwrap();
        barrier2.wait();
    });
    let handle2 = thread::spawn(move || {
        let stream = TcpStream::connect(addr).unwrap();
        registry3
            .register(&stream, Token(1), Interests::READABLE | Interests::WRITABLE)
            .unwrap();
        barrier3.wait();
    });

    poll.poll(&mut events, Some(Duration::from_millis(500)))
        .unwrap();
    assert!(events.iter().count() >= 1);

    // Let the threads return.
    barrier.wait();

    handle1.join().unwrap();
    handle2.join().unwrap();
}

#[test]
fn assertions() {
    fn assert_sync<T: Sync>() {}
    fn assert_send<T: Send>() {}

    assert_sync::<Poll>();
    assert_send::<Poll>();
    assert_sync::<Registry>();
    assert_send::<Registry>();
}
