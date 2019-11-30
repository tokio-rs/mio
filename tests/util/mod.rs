// Not all functions are used by all tests.
#![allow(dead_code, unused_macros)]

use std::net::SocketAddr;
use std::ops::BitOr;
use std::sync::Once;
use std::time::Duration;
use std::{fmt, io};

use log::{error, warn};
use mio::event::Event;
use mio::{Events, Interests, Poll, Token};

pub fn init() {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        env_logger::try_init().expect("unable to initialise logger");
    })
}

pub fn init_with_poll() -> (Poll, Events) {
    init();

    let poll = Poll::new().expect("unable to create Poll instance");
    let events = Events::with_capacity(16);
    (poll, events)
}

pub fn assert_sync<T: Sync>() {}
pub fn assert_send<T: Send>() {}

/// An event that is expected to show up when `Poll` is polled, see
/// `expect_events`.
#[derive(Debug)]
pub struct ExpectEvent {
    token: Token,
    readiness: Readiness,
}

impl ExpectEvent {
    pub fn new<R>(token: Token, readiness: R) -> ExpectEvent
    where
        R: Into<Readiness>,
    {
        ExpectEvent {
            token,
            readiness: readiness.into(),
        }
    }

    fn matches(&self, event: &Event) -> bool {
        event.token() == self.token && self.readiness.matches(event)
    }
}

#[derive(Debug)]
pub struct Readiness(usize);

const READABLE: usize = 0b00_000_001;
const WRITABLE: usize = 0b00_000_010;
const AIO: usize = 0b00_000_100;
const LIO: usize = 0b00_001_000;
const ERROR: usize = 0b00_010_000;
const READ_CLOSED: usize = 0b00_100_000;
const WRITE_CLOSED: usize = 0b01_000_000;
const PRIORITY: usize = 0b10_000_000;

impl Readiness {
    pub const READABLE: Readiness = Readiness(READABLE);
    pub const WRITABLE: Readiness = Readiness(WRITABLE);
    pub const AIO: Readiness = Readiness(AIO);
    pub const LIO: Readiness = Readiness(LIO);
    pub const ERROR: Readiness = Readiness(ERROR);
    pub const READ_CLOSED: Readiness = Readiness(READ_CLOSED);
    pub const WRITE_CLOSED: Readiness = Readiness(WRITE_CLOSED);
    pub const PRIORITY: Readiness = Readiness(PRIORITY);

    fn matches(&self, event: &Event) -> bool {
        // If we expect a readiness then also match on the event.
        // In maths terms that is p -> q, which is the same  as !p || q.
        (!self.is(READABLE) || event.is_readable())
            && (!self.is(WRITABLE) || event.is_writable())
            && (!self.is(AIO) || event.is_aio())
            && (!self.is(LIO) || event.is_lio())
            && (!self.is(ERROR) || event.is_error())
            && (!self.is(READ_CLOSED) || event.is_read_closed())
            && (!self.is(WRITE_CLOSED) || event.is_write_closed())
            && (!self.is(PRIORITY) || event.is_priority())
    }

    /// Usage: `self.is(READABLE)`.
    fn is(&self, value: usize) -> bool {
        self.0 & value != 0
    }
}

impl BitOr for Readiness {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Readiness(self.0 | other.0)
    }
}

impl From<Interests> for Readiness {
    fn from(interests: Interests) -> Readiness {
        let mut readiness = Readiness(0);
        if interests.is_readable() {
            readiness.0 |= READABLE;
        }
        if interests.is_writable() {
            readiness.0 |= WRITABLE;
        }
        if interests.is_aio() {
            readiness.0 |= AIO;
        }
        if interests.is_lio() {
            readiness.0 |= LIO;
        }
        readiness
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
                warn!("got unexpected event: {:?}", event);
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

pub fn expect_no_events(poll: &mut Poll, events: &mut Events) {
    poll.poll(events, Some(Duration::from_millis(50)))
        .expect("unable to poll");
    if !events.is_empty() {
        for event in events.iter() {
            error!("unexpected event: {:?}", event);
        }
        panic!("received events, but didn't expect any, see above");
    }
}

/// Assert that `result` is an error and the formatted error (via
/// `fmt::Display`) equals `expected_msg`.
pub fn assert_error<T, E: fmt::Display>(result: Result<T, E>, expected_msg: &str) {
    match result {
        Ok(_) => panic!("unexpected OK result"),
        Err(err) => assert!(
            err.to_string().contains(expected_msg),
            "wanted: {}, got: {}",
            expected_msg,
            err,
        ),
    }
}

/// Assert that the provided result is an `io::Error` with kind `WouldBlock`.
pub fn assert_would_block<T>(result: io::Result<T>) {
    match result {
        Ok(_) => panic!("unexpected OK result, expected a `WouldBlock` error"),
        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {}
        Err(err) => panic!("unexpected error result: {}", err),
    }
}

/// Bind to any port on localhost.
pub fn any_local_address() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

/// Bind to any port on localhost, using a IPv6 address.
pub fn any_local_ipv6_address() -> SocketAddr {
    "[::1]:0".parse().unwrap()
}

/// A checked {write, send, send_to} macro that ensures the entire buffer is
/// written.
///
/// Usage: `checked_write!(stream.write(&DATA));`
/// Also works for send(_to): `checked_write!(socket.send_to(DATA, address))`.
macro_rules! checked_write {
    ($socket: ident . $method: ident ( $data: expr $(, $arg: expr)* ) ) => {{
        let data = $data;
        let n = $socket.$method($data $(, $arg)*)
            .expect("unable to write to socket");
        assert_eq!(n, data.len(), "short write");
    }};
}

/// A checked {read, recv, recv_from, peek, peek_from} macro that ensures the
/// current buffer is read.
///
/// Usage: `expect_read!(stream.read(&mut buf), DATA);` reads into `buf` and
/// compares it to `DATA`.
/// Also works for recv(_from): `expect_read!(socket.recv_from(&mut buf), DATA, address)`.
macro_rules! expect_read {
    ($socket: ident . $method: ident ( $buf: expr $(, $arg: expr)* ), $expected: expr) => {{
        let n = $socket.$method($buf $(, $arg)*)
            .expect("unable to read from socket");
        let expected = $expected;
        assert_eq!(n, expected.len());
        assert_eq!(&$buf[..n], expected);
    }};
    // TODO: change the call sites to check the source address.
    // Support for recv_from and peek_from, without checking the address.
    ($socket: ident . $method: ident ( $buf: expr $(, $arg: expr)* ), $expected: expr, __anywhere) => {{
        let (n, _address) = $socket.$method($buf $(, $arg)*)
            .expect("unable to read from socket");
        let expected = $expected;
        assert_eq!(n, expected.len());
        assert_eq!(&$buf[..n], expected);
    }};
    // Support for recv_from and peek_from for `UnixDatagram`s.
    ($socket: ident . $method: ident ( $buf: expr $(, $arg: expr)* ), $expected: expr, path: $source: expr) => {{
        let (n, path) = $socket.$method($buf $(, $arg)*)
            .expect("unable to read from socket");
        let expected = $expected;
        let source = $source;
        assert_eq!(n, expected.len());
        assert_eq!(&$buf[..n], expected);
        assert_eq!(
            path.as_pathname().expect("failed to get path name"),
            source
        );
    }};
    // Support for recv_from and peek_from for `UdpSocket`s.
    ($socket: ident . $method: ident ( $buf: expr $(, $arg: expr)* ), $expected: expr, $source: expr) => {{
        let (n, address) = $socket.$method($buf $(, $arg)*)
            .expect("unable to read from socket");
        let expected = $expected;
        let source = $source;
        assert_eq!(n, expected.len());
        assert_eq!(&$buf[..n], expected);
        assert_eq!(address, source);
    }};
}
