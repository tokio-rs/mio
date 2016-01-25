use {sleep_ms};
use mio::*;
use mio::tcp::*;
use std::time::Duration;

const MS: u64 = 1_000;

#[test]
pub fn test_reregister_different_without_poll() {
    let mut poll = Poll::new().unwrap();

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register(&l, Token(0), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    let s1 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();
    poll.register(&s1, Token(2), EventSet::readable(), PollOpt::edge()).unwrap();

    sleep_ms(MS);

    poll.reregister(&l, Token(0), EventSet::writable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    poll.poll(Some(Duration::from_millis(MS))).unwrap();
    assert_eq!(poll.events().len(), 0);
}
