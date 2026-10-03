#![cfg(all(feature = "os-poll", feature = "net"))]

use std::io::{Read, Write};
use std::net;

use mio::net::TcpStream;
use mio::{Interest, Token};

mod util;
use util::{
    any_local_address, assert_would_block, expect_events, expect_no_events, init_with_poll,
    ExpectEvent,
};

const ID1: Token = Token(0);

// Regression tests for #1963.
//
// Re-arming a source after a blocked operation must only re-arm the direction
// that actually blocked. Re-arming everything the source is registered for
// re-requests write readiness after a blocked *read*, and for a writable
// socket that is reported straight back, producing a writable event on every
// blocked read.
//
// `epoll(7)` and `kqueue(2)` are edge triggered and never re-arm, so they
// already hold to this. The selectors that do re-arm all replace the requested
// events rather than adding to them, so narrowing them needs a change per
// selector; only the Windows one is done so far. These tests are skipped on
// the others rather than dropping the invariant:
//
//   - `poll(2)`, via `--cfg mio_unsupported_force_poll_poll` and on WASI
//   - `event_ports(2)`, on Solaris and illumos

/// A blocked read must not raise write readiness.
#[test]
#[cfg_attr(
    any(
        mio_unsupported_force_poll_poll,
        target_os = "solaris",
        target_os = "illumos",
        target_os = "wasi",
    ),
    ignore = "selector re-arms the full interest, see #1963"
)]
fn read_would_block_does_not_produce_a_writable_event() {
    let (mut poll, mut events) = init_with_poll();

    let listener = net::TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();

    let mut stream = TcpStream::connect(addr).unwrap();
    // Hold on to the peer for the duration of the test. It never sends
    // anything, so the stream stays connected but never becomes readable.
    let (_peer, _) = listener.accept().unwrap();

    poll.registry()
        .register(&mut stream, ID1, Interest::READABLE | Interest::WRITABLE)
        .unwrap();

    // The stream is connected, so it is writable.
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interest::WRITABLE)],
    );

    // Nothing has been sent, so this blocks. This is what re-arms the source.
    let mut buf = [0; 16];
    assert_would_block(stream.read(&mut buf));

    // Nothing changed since the writable event was handled: the peer sent
    // nothing, and no write ever blocked. Re-arming the read must not raise
    // write readiness again.
    expect_no_events(&mut poll, &mut events);
}

/// ... and the direction that did block must still be armed afterwards.
///
/// On its own the test above would also pass if the re-arm did nothing at all,
/// so check that the read the source was narrowed to still arrives.
#[test]
#[cfg_attr(
    any(
        mio_unsupported_force_poll_poll,
        target_os = "solaris",
        target_os = "illumos",
        target_os = "wasi",
    ),
    ignore = "selector re-arms the full interest, see #1963"
)]
fn read_would_block_still_arms_the_read() {
    let (mut poll, mut events) = init_with_poll();

    let listener = net::TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();

    let mut stream = TcpStream::connect(addr).unwrap();
    let (mut peer, _) = listener.accept().unwrap();

    poll.registry()
        .register(&mut stream, ID1, Interest::READABLE | Interest::WRITABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interest::WRITABLE)],
    );

    let mut buf = [0; 16];
    assert_would_block(stream.read(&mut buf));

    // Now there is something to read, which the re-armed read interest must
    // still report.
    peer.write_all(b"hello").unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interest::READABLE)],
    );

    let n = stream.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"hello");
}
