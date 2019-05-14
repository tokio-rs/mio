use std::cmp;
use std::io::prelude::*;
use std::io;
use std::net::Shutdown;
use std::thread;
use std::time::Duration;

use net2::{self, TcpStreamExt};

use {TryRead, TryWrite};
use mio::{Token, Ready, PollOpt, Poll, Events};
use iovec::IoVec;
use mio::net::{TcpListener, TcpStream};

macro_rules! wait {
    ($poll:ident, $ready:ident) => {{
        use std::time::Instant;

        let now = Instant::now();
        let mut events = Events::with_capacity(16);

        println!("~~~ WAIT ~~~");

        'outer:
        loop {
            if now.elapsed() > Duration::from_secs(5) {
                panic!("not ready");
            }

            println!(" + poll");
            $poll.poll(&mut events, Some(Duration::from_secs(1))).unwrap();

            for event in &events {
                #[cfg(unix)]
                {
                    use mio::unix::UnixReady;
                    assert!(!event.readiness().is_hup());
                }

                println!("~~~ {:?}", event);
                if event.token() == Token(0) && event.readiness().$ready() {
                    break 'outer
                }
            }
        }
    }};
}

fn setup() -> (Poll, std::net::TcpStream, TcpStream) {
    let poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let mut client = TcpStream::connect(&addr).unwrap();
    poll.register(&client,
                  Token(0),
                  Ready::readable() | Ready::writable(),
                  PollOpt::edge()).unwrap();

    let (socket, _) = listener.accept().unwrap();

    wait!(poll, is_writable);

    (poll, socket, client)
}

#[test]
fn test_write_shutdown() {
    let (poll, socket, client) = setup();
    let mut events = Events::with_capacity(16);

    // Polling should not have any events
    poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
    assert!(events.iter().next().is_none());

    println!("SHUTTING DOWN");
    // Now, shutdown the write half of the socket.
    socket.shutdown(Shutdown::Write).unwrap();

    wait!(poll, is_readable);
}
