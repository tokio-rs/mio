#![cfg(all(feature = "os-poll", feature = "net"))]

use std::io::Read;
use std::net;

use mio::net::TcpStream;
use mio::{Interest, Token};

mod util;
use util::{
    any_local_address, assert_would_block, expect_events, expect_no_events, init_with_poll,
    ExpectEvent,
};

const ID1: Token = Token(0);

/// Regression test for #1963.
///
/// Re-arming a source after a blocked operation must only re-arm the direction
/// that actually blocked. Re-arming everything the source is registered for
/// re-requests write readiness after a blocked *read*, and for a writable
/// socket that is reported straight back, producing a writable event on every
/// blocked read.
///
/// The `poll(2)` selector re-arms the same way and still has this problem:
/// `SelectorState::reregister` overwrites the `pollfd` event mask, so narrowing
/// the re-arm there needs a separate change. It is shared with `event_ports(2)`
/// and left for a follow-up, so this test does not run against it.
#[test]
#[cfg_attr(
    mio_unsupported_force_poll_poll,
    ignore = "the poll(2) selector re-arms the full interest, see #1963"
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
