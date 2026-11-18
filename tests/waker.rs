#![cfg(not(target_os = "wasi"))]
#![cfg(all(feature = "os-poll", feature = "net"))]

use mio::{Events, Poll, Token, Waker};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

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
fn waker_after_drop() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(10);
    let first = Waker::new(poll.registry(), Token(0)).unwrap();
    first.wake().unwrap();
    expect_waker_event(&mut poll, &mut events, Token(0));
    drop(first);

    for token in (1..4).map(Token) {
        let registry = poll.registry().try_clone().unwrap();
        let waker = Waker::new(&registry, token).unwrap();
        drop(registry);

        waker.wake().unwrap();
        expect_waker_event(&mut poll, &mut events, token);
        drop(waker);
    }
}

#[test]
#[cfg(all(
    any(target_os = "linux", target_os = "macos"),
    not(mio_unsupported_force_poll_poll),
))]
#[cfg_attr(miri, ignore = "Miri doesn't support spawning a subprocess")]
fn waker_after_creation_failure() {
    const CHILD: &str = "MIO_TEST_WAKER_CREATION_FAILURE";
    if std::env::var_os(CHILD).is_none() {
        // Changing the fd limit must not interfere with other tests.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "waker_after_creation_failure", "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        return;
    }

    let mut poll = Poll::new().unwrap();
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `limit` is valid storage for the current resource limit.
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
        0
    );
    let exhausted = libc::rlimit {
        rlim_cur: 0,
        rlim_max: limit.rlim_max,
    };
    // SAFETY: only this subprocess's soft limit is changed, and restored below.
    assert_eq!(
        unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &exhausted) },
        0
    );
    let failed = Waker::new(poll.registry(), Token(0));
    // SAFETY: restore the original valid resource limit before checking results.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) }, 0);
    assert!(failed.is_err());

    let waker = Waker::new(poll.registry(), Token(1)).unwrap();
    waker.wake().unwrap();
    let mut events = Events::with_capacity(10);
    expect_waker_event(&mut poll, &mut events, Token(1));
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

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "only works with debug_assertions enabled"
)]
#[should_panic = "Only a single `Waker` can be active per `Poll` instance"]
fn using_multiple_wakers_panics() {
    init();

    let poll = Poll::new().expect("unable to create new Poll instance");
    let token1 = Token(10);
    let token2 = Token(11);

    let waker1 = Waker::new(poll.registry(), token1).expect("unable to first waker");
    // This should panic.
    let waker2 = Waker::new(poll.registry(), token2).unwrap();

    drop(waker1);
    drop(waker2);
}

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "only works with debug_assertions enabled"
)]
#[should_panic = "Only a single `Waker` can be active per `Poll` instance"]
fn using_multiple_wakers_panics_different_cloned_registries() {
    init();

    let poll = Poll::new().expect("unable to create new Poll instance");
    let registry1 = poll.registry().try_clone().unwrap();
    let registry2 = poll.registry().try_clone().unwrap();

    let token1 = Token(10);
    let token2 = Token(11);

    let waker1 = Waker::new(&registry1, token1).expect("unable to first waker");
    // This should panic.
    let waker2 = Waker::new(&registry2, token2).unwrap();

    drop(waker1);
    drop(waker2);
}

fn expect_waker_event(poll: &mut Poll, events: &mut Events, token: Token) {
    poll.poll(events, Some(Duration::from_millis(100))).unwrap();
    assert!(!events.is_empty());
    for event in events.iter() {
        assert_eq!(event.token(), token);
        assert!(event.is_readable());
    }
}
