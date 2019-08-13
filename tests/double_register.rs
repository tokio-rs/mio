//! A smoke test for windows compatibility

#![cfg(any(target_os = "linux", target_os = "windows"))]

use mio::net::TcpListener;
use mio::*;

mod util;

use util::init;

#[test]
pub fn test_double_register() {
    init();

    let poll = Poll::new().unwrap();

    // Create the listener
    let l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.registry()
        .register(&l, Token(0), Interests::READABLE)
        .unwrap();

    assert!(poll
        .registry()
        .register(&l, Token(1), Interests::READABLE)
        .is_err());
}
