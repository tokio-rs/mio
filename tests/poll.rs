#![cfg(not(target_os = "wasi"))]
#![cfg(all(feature = "os-poll", feature = "net"))]

use std::net;
use std::sync::{Arc, Barrier};
use std::thread::{self, sleep};
use std::time::Duration;
use std::{fmt, io};

use mio::event::Source;
use mio::net::{TcpListener, TcpStream, UdpSocket};
use mio::{event, Events, Interest, Poll, Registry, Token};

mod util;
use util::{
    any_local_address, assert_send, assert_sync, expect_events, init, init_with_poll, ExpectEvent,
};

const ID1: Token = Token(1);
const ID2: Token = Token(2);
const ID3: Token = Token(3);

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
    let mut listener = TcpListener::bind(any_local_address()).unwrap();
    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(
            &mut listener,
            Token(1),
            Interest::READABLE | Interest::WRITABLE,
        )
        .unwrap();
    drop(listener);
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

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = thread::spawn(move || {
        let mut stream = listener.incoming().next().unwrap().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set_read_timeout");
        match stream.read(&mut [0; 16]) {
            Ok(_) => (),
            Err(err) => {
                if err.kind() != io::ErrorKind::UnexpectedEof {
                    panic!("{}", err);
                }
            }
        }
    });

    let mut poll = Poll::new().unwrap();
    let mut stream = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(
            &mut stream,
            Token(1),
            Interest::READABLE | Interest::WRITABLE,
        )
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

    let mut buf = [0; 1024];
    match stream.read(&mut buf) {
        Ok(_) => panic!("unexpected ok"),
        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => (),
        Err(err) => panic!("unexpected error: {}", err),
    }

    drop(stream);
    handle.join().unwrap();
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

/// Call all registration operations, ending with `source` being registered with `token` and `final_interests`.
pub fn registry_ops_flow(
    registry: &Registry,
    source: &mut dyn Source,
    token: Token,
    init_interests: Interest,
    final_interests: Interest,
) -> io::Result<()> {
    registry.register(source, token, init_interests).unwrap();
    registry.deregister(source).unwrap();

    registry.register(source, token, init_interests).unwrap();
    registry.reregister(source, token, final_interests)
}

#[test]
fn registry_operations_are_thread_safe() {
    let (mut poll, mut events) = init_with_poll();

    let registry = Arc::new(poll.registry().try_clone().unwrap());
    let registry1 = Arc::clone(&registry);
    let registry2 = Arc::clone(&registry);
    let registry3 = Arc::clone(&registry);

    let barrier = Arc::new(Barrier::new(4));
    let barrier1 = Arc::clone(&barrier);
    let barrier2 = Arc::clone(&barrier);
    let barrier3 = Arc::clone(&barrier);

    let mut listener = TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();

    // Expect that multiple register/deregister/reregister work fine on multiple
    // threads. Main thread will wait before the expect_events for all other 3
    // threads to do their work. Otherwise expect_events timeout might be too short
    // for all threads to complete, and call might fail.

    let handle1 = thread::spawn(move || {
        registry_ops_flow(
            &registry1,
            &mut listener,
            ID1,
            Interest::READABLE,
            Interest::READABLE,
        )
        .unwrap();

        barrier1.wait();
        barrier1.wait();
    });

    let handle2 = thread::spawn(move || {
        let mut udp_socket = UdpSocket::bind(any_local_address()).unwrap();
        registry_ops_flow(
            &registry2,
            &mut udp_socket,
            ID2,
            Interest::WRITABLE,
            Interest::WRITABLE.add(Interest::READABLE),
        )
        .unwrap();

        barrier2.wait();
        barrier2.wait();
    });

    let handle3 = thread::spawn(move || {
        let mut stream = TcpStream::connect(addr).unwrap();
        registry_ops_flow(
            &registry3,
            &mut stream,
            ID3,
            Interest::READABLE,
            Interest::READABLE | Interest::WRITABLE,
        )
        .unwrap();

        barrier3.wait();
        barrier3.wait();
    });

    // wait for threads to finish before expect_events
    barrier.wait();
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID1, Interest::READABLE),
            ExpectEvent::new(ID2, Interest::WRITABLE),
            ExpectEvent::new(ID3, Interest::WRITABLE),
        ],
    );

    // Let the threads return.
    barrier.wait();

    handle1.join().unwrap();
    handle2.join().unwrap();
    handle3.join().unwrap();
}

#[test]
fn register_during_poll() {
    let (mut poll, mut events) = init_with_poll();
    let registry = poll.registry().try_clone().unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let barrier1 = Arc::clone(&barrier);

    let handle1 = thread::spawn(move || {
        let mut stream = UdpSocket::bind(any_local_address()).unwrap();

        barrier1.wait();
        // Get closer to "trying" to register during a poll by doing a short
        // sleep before register to give main thread enough time to start
        // waiting the 5 sec long poll.
        sleep(Duration::from_millis(200));
        registry
            .register(&mut stream, ID1, Interest::WRITABLE)
            .unwrap();

        barrier1.wait();
        drop(stream);
    });

    // Unlock the thread, allow it to register the `UdpSocket`.
    barrier.wait();
    // Concurrently (at least we attempt to) call `Poll::poll`.
    poll.poll(&mut events, Some(Duration::from_secs(5)))
        .unwrap();

    let mut iter = events.iter();
    let event = iter.next().expect("expect an event");
    assert_eq!(event.token(), ID1);
    assert!(event.is_writable());
    assert!(iter.next().is_none(), "unexpected extra event");

    barrier.wait();
    handle1.join().unwrap();
}

// This test checks the following reregister constraints:
// - `reregister` arguments fully override the previous values. In other
// words, if a socket is registered with `READABLE` interest and the call
// to `reregister` specifies `WRITABLE`, then read interest is no longer
// requested for the handle.
// - `reregister` can use the same token as `register`
// - `reregister` can use different token from `register`
// - multiple `reregister` are ok
#[test]
fn reregister_interest_token_usage() {
    let (mut poll, mut events) = init_with_poll();

    let mut udp_socket = UdpSocket::bind(any_local_address()).unwrap();

    poll.registry()
        .register(&mut udp_socket, ID1, Interest::READABLE)
        .expect("unable to register listener");

    poll.registry()
        .reregister(&mut udp_socket, ID1, Interest::READABLE)
        .expect("unable to register listener");

    poll.registry()
        .reregister(&mut udp_socket, ID2, Interest::WRITABLE)
        .expect("unable to register listener");

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID2, Interest::WRITABLE)],
    );
}

// This test checks the following register constraint:
// The event source must **not** have been previously registered with this
// instance of `Poll`, otherwise the behavior is unspecified.
//
// This test is done on Windows and epoll platforms where registering a
// source twice is defined behavior that fail with an error code.
//
// On kqueue platforms registering twice (not *re*registering) works, but that
// is not a test goal, so it is not tested.
#[test]
#[cfg(debug_assertions)] // Check is only present when debug assertions are enabled.
pub fn double_register_different_token() {
    init();
    let poll = Poll::new().unwrap();

    let mut listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();

    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();

    assert_error(
        poll.registry()
            .register(&mut listener, Token(1), Interest::READABLE),
        "already registered",
    );
}

#[test]
fn poll_ok_after_cancelling_pending_ops() {
    let (mut poll, mut events) = init_with_poll();

    let mut listener = TcpListener::bind(any_local_address()).unwrap();
    let address = listener.local_addr().unwrap();

    let registry = Arc::new(poll.registry().try_clone().unwrap());
    let registry1 = Arc::clone(&registry);

    let barrier = Arc::new(Barrier::new(2));
    let barrier1 = Arc::clone(&barrier);

    registry
        .register(&mut listener, ID1, Interest::READABLE)
        .unwrap();

    // Call a dummy poll just to submit an afd poll request
    poll.poll(&mut events, Some(Duration::from_millis(0)))
        .unwrap();

    // This reregister will cancel the previous pending poll op.
    // The token is different from the register done above, so it can ensure
    // the proper event got returned expect_events below.
    registry
        .reregister(&mut listener, ID2, Interest::READABLE)
        .unwrap();

    let handle = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();

        barrier1.wait();

        registry1
            .register(&mut stream, ID3, Interest::WRITABLE)
            .unwrap();

        barrier1.wait();
    });

    // listener ready to accept stream? getting `READABLE` here means the
    // cancelled poll op was cleared, another poll request was submitted
    // which resulted in returning this event
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID2, Interest::READABLE)],
    );

    let (_, _) = listener.accept().unwrap();
    barrier.wait();

    // for the sake of completeness check stream `WRITABLE`
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID3, Interest::WRITABLE)],
    );

    barrier.wait();
    handle.join().expect("unable to join thread");
}

// This test checks the following reregister constraint:
// The event source must have previously been registered with this instance
// of `Poll`, otherwise the behavior is unspecified.
//
// This test is done on Windows and epoll platforms where reregistering a
// source without a previous register is defined behavior that fail with an
// error code.
//
// On kqueue platforms reregistering w/o registering works but that's not a
// test goal, so it is not tested.
#[test]
#[cfg(debug_assertions)] // Check is only present when debug assertions are enabled.
fn reregister_without_register() {
    let poll = Poll::new().expect("unable to create Poll instance");

    let mut listener = TcpListener::bind(any_local_address()).unwrap();

    assert_error(
        poll.registry()
            .reregister(&mut listener, ID1, Interest::READABLE),
        "not registered",
    );
}

// This test checks the following register/deregister constraint:
// The event source must have previously been registered with this instance
// of `Poll`, otherwise the behavior is unspecified.
//
// This test is done on Windows and epoll platforms where deregistering a
// source without a previous register is defined behavior that fail with an
// error code.
//
// On kqueue platforms deregistering w/o registering works but that's not a
// test goal, so it is not tested.
#[test]
#[cfg(debug_assertions)] // Check is only present when debug assertions are enabled.
fn deregister_without_register() {
    let poll = Poll::new().expect("unable to create Poll instance");

    let mut listener = TcpListener::bind(any_local_address()).unwrap();

    assert_error(poll.registry().deregister(&mut listener), "not registered");
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
