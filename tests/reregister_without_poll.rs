use std::time::Duration;

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interests, Poll, Token};

mod util;

use util::sleep_ms;

const MS: u64 = 1_000;

#[test]
pub fn test_reregister_different_without_poll() {
    let mut events = Events::with_capacity(1024);
    let mut poll = Poll::new().unwrap();

    // Create the listener
    let l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.registry()
        .register(&l, Token(0), Interests::READABLE)
        .unwrap();

    let s1 = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    poll.registry()
        .register(&s1, Token(2), Interests::READABLE)
        .unwrap();

    sleep_ms(MS);

    poll.registry()
        .reregister(&l, Token(0), Interests::WRITABLE)
        .unwrap();

    poll.poll(&mut events, Some(Duration::from_millis(MS)))
        .unwrap();
    assert!(events.iter().next().is_none());
}
