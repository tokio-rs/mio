use std::net::Shutdown;
use std::time::Duration;

use mio::net::TcpStream;
use mio::{Events, Interests, Poll, PollOpt, Token};

macro_rules! wait {
    ($poll:ident, $ready:ident) => {{
        use std::time::Instant;

        let now = Instant::now();
        let mut events = Events::with_capacity(16);
        let mut found = false;

        while !found {
            if now.elapsed() > Duration::from_secs(5) {
                panic!("not ready");
            }

            $poll
                .poll(&mut events, Some(Duration::from_secs(1)))
                .unwrap();

            for event in &events {
                #[cfg(unix)]
                {
                    assert!(!event.readiness().is_hup());
                }

                if event.token() == Token(0) && event.readiness().$ready() {
                    found = true;
                    break;
                }
            }
        }
    }};
}

#[test]
fn test_write_shutdown() {
    let mut poll = Poll::new().unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let interests = Interests::readable() | Interests::writable();

    let client = TcpStream::connect(&addr).unwrap();
    poll.registry()
        .register(&client, Token(0), interests, PollOpt::edge())
        .unwrap();

    let (socket, _) = listener.accept().unwrap();

    wait!(poll, is_writable);

    let mut events = Events::with_capacity(16);

    // Polling should not have any events
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
    assert!(events.iter().next().is_none());

    println!("SHUTTING DOWN");
    // Now, shutdown the write half of the socket.
    socket.shutdown(Shutdown::Write).unwrap();

    wait!(poll, is_readable);
}
