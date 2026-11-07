#![cfg(not(target_os = "wasi"))]
#![cfg(all(feature = "os-poll", feature = "net"))]

use std::time::Duration;

use mio::event::{self, Event, Events};
use mio::net::TcpStream;
use mio::{Token, Waker};

mod util;
use util::{assert_send, assert_sync, init_with_poll};

const WAKE_TOKEN: Token = Token(10);

#[test]
fn assert_event_source_implemented_for() {
    fn assert_event_source<E: event::Source>() {}

    assert_event_source::<Box<dyn event::Source>>();
    assert_event_source::<Box<TcpStream>>();
}

#[test]
fn events_all() {
    let (mut poll, mut events) = init_with_poll();
    assert_eq!(events.capacity(), 16);
    assert!(events.is_empty());

    let waker = Waker::new(poll.registry(), WAKE_TOKEN).unwrap();

    waker.wake().expect("unable to wake");
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();

    assert!(!events.is_empty());

    for event in events.iter() {
        assert_eq!(event.token(), WAKE_TOKEN);
        assert!(event.is_readable());
    }

    events.clear();
    assert!(events.is_empty());
}

#[test]
fn is_event_send_sync() {
    assert_send::<Event>();
    assert_sync::<Event>();

    assert_send::<Events>();
    assert_sync::<Events>();
}

#[test]
fn iter_size_hint_and_count() {
    let (mut poll, mut events) = init_with_poll();
    let waker = Waker::new(poll.registry(), WAKE_TOKEN).unwrap();

    waker.wake().expect("unable to wake");
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();

    let total = events.iter().count();
    assert!(total > 0);

    let mut iter = events.iter();
    assert_eq!(iter.size_hint(), (total, Some(total)));
    assert_eq!(iter.by_ref().count(), total);

    let mut iter = events.iter();
    assert!(iter.next().is_some());
    let remaining = total - 1;
    assert_eq!(iter.size_hint(), (remaining, Some(remaining)));
    assert_eq!(iter.count(), remaining);

    let mut iter = events.iter();
    while iter.next().is_some() {}
    // Beyond exhaustion
    assert_eq!(iter.size_hint(), (0, Some(0)));
    assert_eq!(iter.count(), 0);
}
