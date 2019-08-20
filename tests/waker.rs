use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use mio::{Events, Poll, Token, Waker};

mod util;

use util::{assert_send, assert_sync, expect_no_events, init};

#[test]
fn is_send_and_sync() {
    assert_send::<Waker>();
    assert_sync::<Waker>();
}

#[test]
fn waker() {
    init();

    let mut poll = Poll::new().expect("unable to create new Poll instance");
    let mut events = Events::with_capacity(10);

    let token = Token(10);
    let waker = Waker::new(poll.registry(), token).expect("unable to create waker");

    waker.wake().expect("unable to wake");
    expect_waker_event(&mut poll, &mut events, token);
}

#[test]
fn waker_multiple_wakeups_same_thread() {
    init();

    let mut poll = Poll::new().expect("unable to create new Poll instance");
    let mut events = Events::with_capacity(10);

    let token = Token(10);
    let waker = Waker::new(poll.registry(), token).expect("unable to create waker");

    for _ in 0..3 {
        waker.wake().expect("unable to wake");
    }
    expect_waker_event(&mut poll, &mut events, token);
}

#[test]
fn waker_wakeup_different_thread() {
    init();

    let mut poll = Poll::new().expect("unable to create new Poll instance");
    let mut events = Events::with_capacity(10);

    let token = Token(10);
    let waker = Waker::new(poll.registry(), token).expect("unable to create waker");

    let waker = Arc::new(waker);
    let waker1 = Arc::clone(&waker);
    let handle = thread::spawn(move || {
        waker1.wake().expect("unable to wake");
    });

    expect_waker_event(&mut poll, &mut events, token);

    expect_no_events(&mut poll, &mut events);

    handle.join().unwrap();
}

#[test]
fn waker_multiple_wakeups_different_thread() {
    init();

    let mut poll = Poll::new().expect("unable to create new Poll instance");
    let mut events = Events::with_capacity(10);

    let token = Token(10);
    let waker = Waker::new(poll.registry(), token).expect("unable to create waker");
    let waker = Arc::new(waker);
    let waker1 = Arc::clone(&waker);
    let waker2 = Arc::clone(&waker1);

    let handle1 = thread::spawn(move || {
        waker1.wake().expect("unable to wake");
    });

    let barrier = Arc::new(Barrier::new(2));
    let barrier2 = barrier.clone();
    let handle2 = thread::spawn(move || {
        barrier2.wait();
        waker2.wake().expect("unable to wake");
    });

    // Receive the event from thread 1.
    expect_waker_event(&mut poll, &mut events, token);

    // Unblock thread 2.
    barrier.wait();

    // Now we need to receive another event from thread 2.
    expect_waker_event(&mut poll, &mut events, token);

    expect_no_events(&mut poll, &mut events);

    handle1.join().unwrap();
    handle2.join().unwrap();
}

fn expect_waker_event(poll: &mut Poll, events: &mut Events, token: Token) {
    poll.poll(events, Some(Duration::from_millis(100))).unwrap();
    assert!(!events.is_empty());
    for event in events.iter() {
        assert_eq!(event.token(), token);
        assert!(event.is_readable());
    }
}
