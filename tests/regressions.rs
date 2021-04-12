#![cfg(all(feature = "os-poll", feature = "net"))]

use std::io::{self, Read};
use std::sync::Arc;
use std::time::Duration;
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

    let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = thread::spawn(move || {
        let mut stream = listener.accept().expect("accept").0;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set_read_timeout");
        let _ = stream.read(&mut [0; 16]).expect("read");
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

#[test]
#[cfg(unix)]
fn issue_1403() {
    use mio::net::UnixDatagram;
    use util::temp_file;

    init();

    let path = temp_file("issue_1403");
    let datagram1 = UnixDatagram::bind(&path).unwrap();
    let datagram2 = UnixDatagram::unbound().unwrap();

    let mut buf = [1u8; 1024];
    let n = datagram2.send_to(&buf, &path).unwrap();

    let (got, addr) = datagram1.recv_from(&mut buf).unwrap();
    assert_eq!(got, n);
    assert_eq!(addr.as_pathname(), None);
}
