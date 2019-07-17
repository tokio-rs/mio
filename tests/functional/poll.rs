use std::time::Duration;

use mio::{Events, Poll};

#[test]
fn test_poll_closes_fd() {
    for _ in 0..2000 {
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(4);

        poll.poll(&mut events, Some(Duration::from_millis(0)))
            .unwrap();

        drop(poll);
    }
}

#[test]
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn double_register() {
    use crate::util::localhost;
    use mio::net::TcpListener;

    let poll = Poll::new().unwrap();

    // Create the listener.
    let l = TcpListener::bind(localhost()).unwrap();

    poll.registry()
        .register(&l, Token(0), Interests::READABLE)
        .unwrap();

    assert!(poll
        .registry()
        .register(&l, Token(1), Interests::READABLE)
        .is_err());
}
