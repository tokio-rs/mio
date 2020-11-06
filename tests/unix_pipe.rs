#![cfg(all(unix, feature = "os-poll", feature = "os-ext"))]

use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use mio::event::Event;
use mio::unix::pipe::{self, Receiver, Sender};
use mio::{Events, Interest, Poll, Token};

const RECEIVER: Token = Token(0);
const SENDER: Token = Token(1);

const DATA1: &[u8; 11] = b"Hello world";

#[test]
fn smoke() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);

    let (mut sender, mut receiver) = pipe::new().unwrap();

    let mut buf = [0; 20];
    assert_would_block(receiver.read(&mut buf));

    poll.registry()
        .register(&mut receiver, RECEIVER, Interest::READABLE)
        .unwrap();
    poll.registry()
        .register(&mut sender, SENDER, Interest::WRITABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(SENDER, Interest::WRITABLE)],
    );
    let n = sender.write(DATA1).unwrap();
    assert_eq!(n, DATA1.len());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(RECEIVER, Interest::READABLE)],
    );
    let n = receiver.read(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(&buf[..n], &*DATA1);
}

#[test]
fn event_when_sender_is_dropped() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);

    let (mut sender, mut receiver) = pipe::new().unwrap();
    poll.registry()
        .register(&mut receiver, RECEIVER, Interest::READABLE)
        .unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let thread_barrier = barrier.clone();

    let handle = thread::spawn(move || {
        let n = sender.write(DATA1).unwrap();
        assert_eq!(n, DATA1.len());
        thread_barrier.wait();

        thread_barrier.wait();
        drop(sender);
        thread_barrier.wait();
    });

    barrier.wait(); // Wait for the write to complete.
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(RECEIVER, Interest::READABLE)],
    );

    barrier.wait(); // Unblock the thread.
    barrier.wait(); // Wait until the sending end is dropped.

    expect_one_closed_event(&mut poll, &mut events, RECEIVER, true);

    handle.join().unwrap();
}

#[test]
fn event_when_receiver_is_dropped() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);

    let (mut sender, receiver) = pipe::new().unwrap();
    poll.registry()
        .register(&mut sender, SENDER, Interest::WRITABLE)
        .unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let thread_barrier = barrier.clone();

    let handle = thread::spawn(move || {
        thread_barrier.wait();
        drop(receiver);
        thread_barrier.wait();
    });

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(SENDER, Interest::WRITABLE)],
    );

    barrier.wait(); // Unblock the thread.
    barrier.wait(); // Wait until the receiving end is dropped.

    expect_one_closed_event(&mut poll, &mut events, SENDER, false);

    handle.join().unwrap();
}

#[test]
fn from_child_process_io() {
    // `cat` simply echo everything that we write via standard in.
    let mut child = Command::new("cat")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to start `cat` command");

    let mut sender = Sender::from(child.stdin.take().unwrap());
    let mut receiver = Receiver::from(child.stdout.take().unwrap());

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);

    poll.registry()
        .register(&mut receiver, RECEIVER, Interest::READABLE)
        .unwrap();
    poll.registry()
        .register(&mut sender, SENDER, Interest::WRITABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(SENDER, Interest::WRITABLE)],
    );
    let n = sender.write(DATA1).unwrap();
    assert_eq!(n, DATA1.len());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(RECEIVER, Interest::READABLE)],
    );
    let mut buf = [0; 20];
    let n = receiver.read(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(&buf[..n], &*DATA1);

    drop(sender);

    expect_one_closed_event(&mut poll, &mut events, RECEIVER, true);

    child.wait().unwrap();
}

#[test]
fn nonblocking_child_process_io() {
    // `cat` simply echo everything that we write via standard in.
    let mut child = Command::new("cat")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to start `cat` command");

    let sender = Sender::from(child.stdin.take().unwrap());
    let mut receiver = Receiver::from(child.stdout.take().unwrap());

    receiver.set_nonblocking(true).unwrap();

    let mut buf = [0; 20];
    assert_would_block(receiver.read(&mut buf));

    drop(sender);
    child.wait().unwrap();
}

/// An event that is expected to show up when `Poll` is polled, see
/// `expect_events`.
#[derive(Debug)]
pub struct ExpectEvent {
    token: Token,
    interests: Interest,
}

impl ExpectEvent {
    pub const fn new(token: Token, interests: Interest) -> ExpectEvent {
        ExpectEvent { token, interests }
    }

    fn matches(&self, event: &Event) -> bool {
        event.token() == self.token &&
            // If we expect a readiness then also match on the event.
            // In maths terms that is p -> q, which is the same  as !p || q.
            (!self.interests.is_readable() || event.is_readable()) &&
            (!self.interests.is_writable() || event.is_writable()) &&
            (!self.interests.is_aio() || event.is_aio()) &&
            (!self.interests.is_lio() || event.is_lio())
    }
}

pub fn expect_events(poll: &mut Poll, events: &mut Events, mut expected: Vec<ExpectEvent>) {
    // In a lot of calls we expect more then one event, but it could be that
    // poll returns the first event only in a single call. To be a bit more
    // lenient we'll poll a couple of times.
    for _ in 0..3 {
        poll.poll(events, Some(Duration::from_millis(500)))
            .expect("unable to poll");

        for event in events.iter() {
            let index = expected.iter().position(|expected| expected.matches(event));

            if let Some(index) = index {
                expected.swap_remove(index);
            } else {
                // Must accept sporadic events.
                println!("got unexpected event: {:?}", event);
            }
        }

        if expected.is_empty() {
            return;
        }
    }

    assert!(
        expected.is_empty(),
        "the following expected events were not found: {:?}",
        expected
    );
}

/// Assert that the provided result is an `io::Error` with kind `WouldBlock`.
pub fn assert_would_block<T>(result: io::Result<T>) {
    match result {
        Ok(_) => panic!("unexpected OK result, expected a `WouldBlock` error"),
        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {}
        Err(err) => panic!("unexpected error result: {}", err),
    }
}

/// Expected a closed event. If `read` is true is checks for `is_read_closed`,
/// otherwise for `is_write_closed`.
pub fn expect_one_closed_event(poll: &mut Poll, events: &mut Events, token: Token, read: bool) {
    poll.poll(events, Some(Duration::from_secs(1))).unwrap();
    let mut iter = events.iter();
    let event = iter.next().unwrap();
    assert_eq!(event.token(), token, "invalid token, event: {:#?}", event);
    if read {
        assert!(
            event.is_read_closed(),
            "expected closed or error, event: {:#?}",
            event
        );
    } else {
        assert!(
            event.is_write_closed(),
            "expected closed or error, event: {:#?}",
            event
        );
    }
    assert!(iter.next().is_none());
}
