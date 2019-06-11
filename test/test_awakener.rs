use std::sync::{Arc, Barrier};
use std::thread;

use mio::event::Event;
use mio::{Awakener, Events, Poll, Ready, Token};

use super::{expect_events, expect_no_events};

#[test]
fn awakener() {
    let mut poll = Poll::new().expect("unable to create new Poll instance");
    let mut events = Events::with_capacity(10);

    let token = Token(10);
    let awakener = Awakener::new(poll.registry(), token).expect("unable to create awakener");

    awakener.wake().expect("unable to wake");
    expect_events(
        &mut poll,
        &mut events,
        1,
        vec![Event::new(Ready::READABLE, token)],
    );
}

#[test]
fn awakener_multiple_wakeups_same_thread() {
    let mut poll = Poll::new().expect("unable to create new Poll instance");
    let mut events = Events::with_capacity(10);

    let token = Token(10);
    let awakener = Awakener::new(poll.registry(), token).expect("unable to create awakener");

    for _ in 0..3 {
        awakener.wake().expect("unable to wake");
    }
    expect_events(
        &mut poll,
        &mut events,
        1,
        vec![Event::new(Ready::READABLE, token)],
    );
}

#[test]
fn awakener_wakeup_different_thread() {
    let mut poll = Poll::new().expect("unable to create new Poll instance");
    let mut events = Events::with_capacity(10);

    let token = Token(10);
    let awakener = Awakener::new(poll.registry(), token).expect("unable to create awakener");

    let awakener = Arc::new(awakener);
    let awakener1 = Arc::clone(&awakener);
    let handle = thread::spawn(move || {
        awakener1.wake().expect("unable to wake");
    });

    expect_events(
        &mut poll,
        &mut events,
        1,
        vec![Event::new(Ready::READABLE, token)],
    );

    expect_no_events(&mut poll, &mut events);

    handle.join().unwrap();
}

#[test]
fn awakener_multiple_wakeups_different_thread() {
    let mut poll = Poll::new().expect("unable to create new Poll instance");
    let mut events = Events::with_capacity(10);

    let token = Token(10);
    let awakener = Awakener::new(poll.registry(), token).expect("unable to create awakener");
    let awakener = Arc::new(awakener);
    let awakener1 = Arc::clone(&awakener);
    let awakener2 = Arc::clone(&awakener1);

    let handle1 = thread::spawn(move || {
        awakener1.wake().expect("unable to wake");
    });

    let barrier = Arc::new(Barrier::new(2));
    let barrier2 = barrier.clone();
    let handle2 = thread::spawn(move || {
        barrier2.wait();
        awakener2.wake().expect("unable to wake");
    });

    // Receive the event from thread 1.
    expect_events(
        &mut poll,
        &mut events,
        1,
        vec![Event::new(Ready::READABLE, token)],
    );

    // Unblock thread 2.
    barrier.wait();

    // Now we need to receive another event from thread 2.
    expect_events(
        &mut poll,
        &mut events,
        1,
        vec![Event::new(Ready::READABLE, token)],
    );

    expect_no_events(&mut poll, &mut events);

    handle1.join().unwrap();
    handle2.join().unwrap();
}
