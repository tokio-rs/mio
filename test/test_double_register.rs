//! A smoke test for windows compatibility

#[test]
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn test_double_register() {
    use mio::*;
    use mio::net::TcpListener;

    let poll = Poll::new().unwrap();

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register().register(&l, Token(0), Ready::READABLE, PollOpt::EDGE).unwrap();
    assert!(poll.register().register(&l, Token(1), Ready::READABLE, PollOpt::EDGE).is_err());
}
