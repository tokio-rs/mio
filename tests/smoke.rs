use mio;

use mio::net::TcpListener;
use mio::{Events, Interests, Poll, Token};
use std::time::Duration;

mod util;

use util::init;

#[test]
fn run_once_with_nothing() {
    init();

    let mut events = Events::with_capacity(1024);
    let mut poll = Poll::new().unwrap();
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
}

#[test]
fn add_then_drop() {
    init();

    let mut events = Events::with_capacity(1024);
    let l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&l, Token(1), Interests::READABLE | Interests::WRITABLE)
        .unwrap();
    drop(l);
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
}
