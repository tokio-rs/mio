use {sleep_ms};
use mio::*;
use mio::net::{TcpListener, TcpStream};
use std::time::Duration;

const MS: u64 = 1_000;

#[test]
pub fn test_reregister_different_without_poll() {
    let mut events = Events::with_capacity(1024);
    let mut poll = Poll::new().unwrap();

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register().register(&l, Token(0), Ready::READABLE, PollOpt::EDGE | PollOpt::ONESHOT).unwrap();

    let s1 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();
    poll.register().register(&s1, Token(2), Ready::READABLE, PollOpt::EDGE).unwrap();

    sleep_ms(MS);

    poll.register().reregister(&l, Token(0), Ready::WRITABLE, PollOpt::EDGE | PollOpt::ONESHOT).unwrap();

    poll.poll(&mut events, Some(Duration::from_millis(MS))).unwrap();
    assert!(events.iter().next().is_none());
}
