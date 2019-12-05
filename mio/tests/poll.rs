#![cfg(all(feature = "os-poll", feature = "tcp"))]

use mio::net::{TcpListener, TcpStream};
use mio::{event, Events, Interest, Poll, Registry, Token};
use std::net;
use std::sync::{Arc, Barrier};
use std::thread::{self, sleep};
use std::time::Duration;
use std::{fmt, io};

mod util;
use util::{any_local_address, assert_send, assert_sync, init};

#[test]
fn is_send_and_sync() {
    assert_send::<Events>();
    assert_sync::<Events>();

    assert_sync::<Poll>();
    assert_send::<Poll>();

    assert_sync::<Registry>();
    assert_send::<Registry>();
}

#[test]
fn run_once_with_nothing() {
    init();

    let mut events = Events::with_capacity(16);
    let mut poll = Poll::new().unwrap();
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
}

#[test]
fn add_then_drop() {
    init();

    let mut events = Events::with_capacity(16);
    let mut l = TcpListener::bind(any_local_address()).unwrap();
    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut l, Token(1), Interest::READABLE | Interest::WRITABLE)
        .unwrap();
    drop(l);
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
}

#[test]
fn zero_duration_polls_events() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);

    let listener = net::TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();

    let streams: Vec<TcpStream> = (0..3)
        .map(|n| {
            let mut stream = TcpStream::connect(addr).unwrap();
            poll.registry()
                .register(&mut stream, Token(n), Interest::WRITABLE)
                .unwrap();
            stream
        })
        .collect();

    // Ensure the TcpStreams have some time to connection and for the events to
    // show up.
    sleep(Duration::from_millis(10));

    // Even when passing a zero duration timeout we still want do the system
    // call.
    poll.poll(&mut events, Some(Duration::from_nanos(0)))
        .unwrap();
    assert!(!events.is_empty());

    // Both need to live until here.
    drop(streams);
    drop(listener);
}

#[test]
fn poll_closes_fd() {
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
fn drop_cancels_interest_and_shuts_down() {
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
fn registry_behind_arc() {
    // `Registry` should work behind an `Arc`, being `Sync` and `Send`.
    init();

    let mut poll = Poll::new().unwrap();
    let registry = Arc::new(poll.registry().try_clone().unwrap());
    let mut events = Events::with_capacity(128);

    let mut listener = TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();
    let barrier = Arc::new(Barrier::new(3));

    let registry2 = Arc::clone(&registry);
    let registry3 = Arc::clone(&registry);
    let barrier2 = Arc::clone(&barrier);
    let barrier3 = Arc::clone(&barrier);

    let handle1 = thread::spawn(move || {
        registry2
            .register(&mut listener, Token(0), Interest::READABLE)
            .unwrap();
        barrier2.wait();
    });
    let handle2 = thread::spawn(move || {
        let mut stream = TcpStream::connect(addr).unwrap();
        registry3
            .register(
                &mut stream,
                Token(1),
                Interest::READABLE | Interest::WRITABLE,
            )
            .unwrap();
        barrier3.wait();
    });

    poll.poll(&mut events, Some(Duration::from_millis(1000)))
        .unwrap();
    assert!(events.iter().count() >= 1);

    // Let the threads return.
    barrier.wait();

    handle1.join().unwrap();
    handle2.join().unwrap();
}

// On kqueue platforms registering twice (not *re*registering) works.
#[test]
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn double_register() {
    init();
    let poll = Poll::new().unwrap();

    let mut l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();

    poll.registry()
        .register(&mut l, Token(0), Interest::READABLE)
        .unwrap();

    assert!(poll
        .registry()
        .register(&mut l, Token(1), Interest::READABLE)
        .is_err());
}

struct TestEventSource {
    registrations: Vec<(Token, Interest)>,
    reregistrations: Vec<(Token, Interest)>,
    deregister_count: usize,
}

impl TestEventSource {
    fn new() -> TestEventSource {
        TestEventSource {
            registrations: Vec::new(),
            reregistrations: Vec::new(),
            deregister_count: 0,
        }
    }
}

impl event::Source for TestEventSource {
    fn register(
        &mut self,
        _registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.registrations.push((token, interests));
        Ok(())
    }

    fn reregister(
        &mut self,
        _registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.reregistrations.push((token, interests));
        Ok(())
    }

    fn deregister(&mut self, _registry: &Registry) -> io::Result<()> {
        self.deregister_count += 1;
        Ok(())
    }
}

#[test]
fn poll_registration() {
    init();
    let poll = Poll::new().unwrap();
    let registry = poll.registry();

    let mut source = TestEventSource::new();
    let token = Token(0);
    let interests = Interest::READABLE;
    registry.register(&mut source, token, interests).unwrap();
    assert_eq!(source.registrations.len(), 1);
    assert_eq!(source.registrations.get(0), Some(&(token, interests)));
    assert!(source.reregistrations.is_empty());
    assert_eq!(source.deregister_count, 0);

    let re_token = Token(0);
    let re_interests = Interest::READABLE;
    registry
        .reregister(&mut source, re_token, re_interests)
        .unwrap();
    assert_eq!(source.registrations.len(), 1);
    assert_eq!(source.reregistrations.len(), 1);
    assert_eq!(
        source.reregistrations.get(0),
        Some(&(re_token, re_interests))
    );
    assert_eq!(source.deregister_count, 0);

    registry.deregister(&mut source).unwrap();
    assert_eq!(source.registrations.len(), 1);
    assert_eq!(source.reregistrations.len(), 1);
    assert_eq!(source.deregister_count, 1);
}

struct ErroneousTestEventSource;

impl event::Source for ErroneousTestEventSource {
    fn register(
        &mut self,
        _registry: &Registry,
        _token: Token,
        _interests: Interest,
    ) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "register"))
    }

    fn reregister(
        &mut self,
        _registry: &Registry,
        _token: Token,
        _interests: Interest,
    ) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "reregister"))
    }

    fn deregister(&mut self, _registry: &Registry) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "deregister"))
    }
}

#[test]
fn poll_erroneous_registration() {
    init();
    let poll = Poll::new().unwrap();
    let registry = poll.registry();

    let mut source = ErroneousTestEventSource;
    let token = Token(0);
    let interests = Interest::READABLE;
    assert_error(registry.register(&mut source, token, interests), "register");
    assert_error(
        registry.reregister(&mut source, token, interests),
        "reregister",
    );
    assert_error(registry.deregister(&mut source), "deregister");
}

/// Assert that `result` is an error and the formatted error (via
/// `fmt::Display`) equals `expected_msg`.
pub fn assert_error<T, E: fmt::Display>(result: Result<T, E>, expected_msg: &str) {
    match result {
        Ok(_) => panic!("unexpected OK result"),
        Err(err) => assert!(
            err.to_string().contains(expected_msg),
            "wanted: {}, got: {}",
            err,
            expected_msg
        ),
    }
}
