use mio::*;
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[test]
fn test_poll_closes_fd() {
    for _ in 0..2000 {
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(4);
        let (registration, set_readiness) = Registration::new2();

        poll.register(&registration, Token(0), Ready::readable(), PollOpt::edge()).unwrap();
        poll.poll(&mut events, Some(Duration::from_millis(0))).unwrap();

        drop(poll);
        drop(set_readiness);
        drop(registration);
    }
}

#[test]
fn test_poll_duration_none_blocks() {
    let is_blocked = Arc::new(AtomicBool::new(true));

    let poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(4);
    let (registration, _) = Registration::new2();

    poll.register(&registration, Token(0), Ready::readable(), PollOpt::edge()).unwrap();

    let is_blocked2 = is_blocked.clone();
    let _join = thread::spawn(move || {
        poll.poll(&mut events, None).unwrap();
        is_blocked2.store(false, Ordering::SeqCst);
    });

    assert!(is_blocked.load(Ordering::SeqCst));

}

#[test]
fn test_poll_duration_0_doesnt_block() {
    let is_blocked = Arc::new(AtomicBool::new(true));

    let poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(4);
    let (registration, _) = Registration::new2();

    poll.register(&registration, Token(0), Ready::readable(), PollOpt::edge()).unwrap();

    let is_blocked2 = is_blocked.clone();
    let _join = thread::spawn(move || {
        poll.poll(&mut events, Some(Duration::new(0, 0))).unwrap();
        is_blocked2.store(false, Ordering::SeqCst);
    });

    thread::sleep(Duration::from_millis(1));
    assert_eq!(is_blocked.load(Ordering::SeqCst), false);

}

#[test]
fn test_poll_unblocks() {
    let is_blocked = Arc::new(AtomicBool::new(true));

    let poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(4);
    let (registration, set_readiness) = Registration::new2();

    poll.register(&registration, Token(0), Ready::readable(), PollOpt::edge()).unwrap();

    let is_blocked2 = is_blocked.clone();
    let _join = thread::spawn(move || {
        poll.poll(&mut events, None).unwrap();
        is_blocked2.store(false, Ordering::SeqCst);
    });

    assert!(is_blocked.load(Ordering::SeqCst));
    set_readiness.set_readiness(Ready::readable()).unwrap();
    thread::sleep(Duration::from_millis(1));
    assert_eq!(is_blocked.load(Ordering::SeqCst), false);

}


#[test]
fn test_poll_timeout() {
    let is_blocked = Arc::new(AtomicBool::new(true));

    let poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(4);
    let (registration, _) = Registration::new2();

    poll.register(&registration, Token(0), Ready::readable(), PollOpt::edge()).unwrap();

    let is_blocked2 = is_blocked.clone();
    let _join = thread::spawn(move || {
        poll.poll(&mut events, Some(Duration::from_millis(500))).unwrap();
        is_blocked2.store(false, Ordering::SeqCst);
    });

    assert!(is_blocked.load(Ordering::SeqCst));
    thread::sleep(Duration::from_millis(600));
    assert_eq!(is_blocked.load(Ordering::SeqCst), false);

}

