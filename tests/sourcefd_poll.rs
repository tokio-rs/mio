#![cfg(all(unix, feature = "os-poll", feature = "os-ext"))]

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::time::Duration;

use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};

// Regression test for https://github.com/tokio-rs/mio/issues/2018.
//
// A fd registered with `SourceFd` must keep reporting readiness on every
// round, not just the first one. The `poll(2)` selector used to clear the
// interest after delivering an event and only `IoSourceState::do_io` added
// it back; `SourceFd` never goes through that wrapper, so later events
// were lost.
#[test]
fn source_fd_reports_readiness_on_every_round() {
    let mut fds = [0; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    unsafe { libc::fcntl(fds[0], libc::F_SETFL, libc::O_NONBLOCK) };
    let mut rx = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    let mut tx = unsafe { std::fs::File::from_raw_fd(fds[1]) };

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut SourceFd(&rx.as_raw_fd()), Token(0), Interest::READABLE)
        .unwrap();
    let mut events = Events::with_capacity(8);

    for _ in 0..3 {
        tx.write_all(b"x").unwrap();
        poll.poll(&mut events, Some(Duration::from_millis(500)))
            .unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.token() == Token(0) && e.is_readable()),
            "expected a readable event in every round"
        );
        // Drain the pipe completely, until WouldBlock, as edge-triggered
        // use requires.
        let mut buf = [0u8; 64];
        loop {
            match rx.read(&mut buf) {
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("unexpected read error: {e}"),
            }
        }
    }
}
