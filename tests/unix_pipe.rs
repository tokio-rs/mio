#![cfg(all(unix, feature = "os-poll", feature = "os-ext", feature = "net"))]

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use mio::unix::pipe::{self, Receiver, Sender};
use mio::{Events, Interest, Poll, Token};

mod util;
use util::{assert_would_block, expect_events, ExpectEvent};

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
