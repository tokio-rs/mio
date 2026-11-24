//! QNX: every registration takes one of the process's registered events,
//! capped by `RLIMIT_SIGEVENT_NP`. Running out must fail `register` cleanly
//! and leave everything already registered working.

#![cfg(all(
    target_os = "qnx",
    not(mio_unsupported_force_poll_poll),
    feature = "os-poll",
    feature = "net"
))]

use std::net;

use mio::net::UdpSocket;
use mio::{Interest, Token};

mod util;
use util::{any_local_address, expect_events, init_with_poll, ExpectEvent};

/// From `sys/resource.h`; libc doesn't define it.
const RLIMIT_SIGEVENT_NP: libc::c_int = 12;
/// Small enough to keep the test quick and well below the fd limit.
const LIMIT: libc::rlim_t = 64;

#[test]
fn register_until_event_limit() {
    let (mut poll, mut events) = init_with_poll();

    // Lowering the soft limit needs no privileges.
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `limit` is valid for writes.
    assert_eq!(
        unsafe { libc::getrlimit(RLIMIT_SIGEVENT_NP, &mut limit) },
        0
    );
    limit.rlim_cur = limit.rlim_cur.min(LIMIT);
    // SAFETY: `limit` is a valid rlimit.
    assert_eq!(unsafe { libc::setrlimit(RLIMIT_SIGEVENT_NP, &limit) }, 0);

    // Register sockets until the limit is hit.
    let mut sockets = Vec::new();
    let err = loop {
        let mut socket = UdpSocket::bind(any_local_address()).unwrap();
        let token = Token(sockets.len());
        match poll
            .registry()
            .register(&mut socket, token, Interest::READABLE)
        {
            Ok(()) => sockets.push(socket),
            Err(err) => break err,
        }
        assert!(sockets.len() as libc::rlim_t <= LIMIT, "limit not enforced");
    };
    assert_eq!(err.raw_os_error(), Some(libc::EAGAIN), "{err}");
    assert!(!sockets.is_empty());

    // Freeing one registration makes room for exactly one more. A fresh
    // socket is used: in debug builds a source whose `register` failed still
    // counts as associated with the registry.
    poll.registry().deregister(&mut sockets[0]).unwrap();
    let mut spare = UdpSocket::bind(any_local_address()).unwrap();
    poll.registry()
        .register(&mut spare, Token(0), Interest::READABLE)
        .unwrap();
    sockets[0] = spare;
    let mut extra = UdpSocket::bind(any_local_address()).unwrap();
    let err = poll
        .registry()
        .register(&mut extra, Token(sockets.len()), Interest::READABLE)
        .unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::EAGAIN), "{err}");

    // What is registered still reports readiness.
    let last = sockets.len() - 1;
    let sender = net::UdpSocket::bind(any_local_address()).unwrap();
    for socket in [&sockets[0], &sockets[last]] {
        sender
            .send_to(b"ping", socket.local_addr().unwrap())
            .unwrap();
    }
    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(Token(0), Interest::READABLE),
            ExpectEvent::new(Token(last), Interest::READABLE),
        ],
    );

    // Deregistering gives every event back: the same number fits again.
    for socket in &mut sockets {
        poll.registry().deregister(socket).unwrap();
    }
    for (i, socket) in sockets.iter_mut().enumerate() {
        poll.registry()
            .register(socket, Token(i), Interest::READABLE)
            .unwrap();
    }
}
