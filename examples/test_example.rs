macro_rules! checked_write {
    ($socket: ident . $method: ident ( $data: expr $(, $arg: expr)* ) ) => {{
        let data = $data;
        let n = $socket.$method($data $(, $arg)*)
            .expect("unable to write to socket");
        assert_eq!(n, data.len(), "short write");
    }};
}

/// Assert that the provided result is an `io::Error` with kind `WouldBlock`.
pub fn assert_would_block<T>(result: std::io::Result<T>) {
    match result {
        Ok(_) => panic!("unexpected OK result, expected a `WouldBlock` error"),
        Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(err) => panic!("unexpected error result: {}", err),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use mio::{net, net::TcpStream, Events, Interest, Poll, Token, Waker};
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    env_logger::init();

    const DATA1: &[u8] = b"Hello world!";
    const ID1: Token = Token(1);

    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024);

    let listener = net::TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let sockaddr = listener.local_addr().unwrap();
    let mut stream = TcpStream::connect(sockaddr).unwrap();

    poll.registry()
        .register(&mut stream, ID1, Interest::READABLE.add(Interest::WRITABLE))
        .unwrap();

    let server_stream = listener.accept().unwrap();

    poll.poll(&mut events, None)?;

    /* expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interest::WRITABLE)],
    ); */
    checked_write!(stream.write(DATA1));

    // Try to read something.
    assert_would_block(stream.read(&mut [0]));

    // Server goes away.
    drop(server_stream);

    poll.poll(&mut events, None)?;

    /* expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Readiness::READ_CLOSED)],
    ); */

    // Make sure we quiesce. `expect_no_events` seems to flake sometimes on mac/freebsd.
    loop {
        poll.poll(&mut events, Some(Duration::from_millis(100)))
            .expect("poll failed");
        if events.iter().count() == 0 {
            break;
        }
    }

    Ok(())
}
