use std::collections::HashMap;
use std::net::Shutdown;
use std::ops::Deref;
use std::time::{Duration, Instant};

use mio::{Token, Ready, PollOpt, Poll, Evented, Events};
use mio::net::TcpStream;

struct TestPoll {
    poll: Poll,
    events: Events,
    buf: HashMap<Token, Ready>,
}

impl TestPoll {
    fn new() -> TestPoll {
        TestPoll {
            poll: Poll::new().unwrap(),
            events: Events::with_capacity(1024),
            buf: HashMap::new(),
        }
    }

    fn register<E: ?Sized>(&self, handle: &E, token: Token, interest: Ready, opts: PollOpt)
        where E: Evented
    {
        self.poll.register(handle, token, interest, opts).unwrap();
    }

    fn wait_for(&mut self, token: Token, ready: Ready) {
        let now = Instant::now();

        loop {
            if now.elapsed() > Duration::from_secs(5) {
                panic!("not ready");
            }

            if let Some(curr) = self.buf.get(&token) {
                if curr.contains(ready) {
                    break;
                }
            }

            self.poll.poll(&mut self.events, Some(Duration::from_secs(1))).unwrap();

            for event in &self.events {
                let curr = self.buf.entry(event.token())
                    .or_insert(Ready::empty());

                *curr |= event.readiness();
            }
        }

        *self.buf.get_mut(&token).unwrap() -= ready;
    }

    fn assert_idle(&mut self) {
        self.poll.poll(&mut self.events, Some(Duration::from_millis(100))).unwrap();

        if let Some(e) = self.events.iter().next() {
            panic!("expected idle; got = {:?}", e);
        }
    }

    fn readiness(&self, token: Token) -> Ready {
        self.buf.get(&token).map(|r| *r).unwrap_or(Ready::empty())
    }
}

macro_rules! wait_for_hup {
    ($poll:expr) => {
        #[cfg(unix)]
        {
            use mio::unix::UnixReady;
            $poll.wait_for(Token(0), UnixReady::hup().into());
        }
    }
}

#[test]
fn test_write_shutdown() {
    use std::io::prelude::*;

    let mut poll = TestPoll::new();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let mut client = TcpStream::connect(&addr).unwrap();
    poll.register(&client,
                  Token(0),
                  Ready::readable() | Ready::writable(),
                  PollOpt::edge());

    let (socket, _) = listener.accept().unwrap();

    poll.wait_for(Token(0), Ready::writable());

    // Polling should not have any events
    poll.assert_idle();

    // Now, shutdown the write half of the socket.
    socket.shutdown(Shutdown::Write).unwrap();

    poll.wait_for(Token(0), Ready::readable());

    #[cfg(unix)]
    {
        use mio::unix::UnixReady;

        let readiness = poll.readiness(Token(0));
        assert!(!UnixReady::from(readiness).is_hup());
    }

    let mut buf = [0; 1024];
    let n = client.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn test_graceful_shutdown() {
    use std::io::prelude::*;

    let mut poll = TestPoll::new();
    let mut buf = [0; 1024];

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let mut client = TcpStream::connect(&addr).unwrap();
    poll.register(&client,
                  Token(0),
                  Ready::readable() | Ready::writable(),
                  PollOpt::edge());

    let (mut socket, _) = listener.accept().unwrap();

    poll.wait_for(Token(0), Ready::writable());

    // Polling should not have any events
    poll.assert_idle();

    // Now, shutdown the write half of the socket.
    client.shutdown(Shutdown::Write).unwrap();

    let n = socket.read(&mut buf).unwrap();
    assert_eq!(0, n);
    drop(socket);

    poll.wait_for(Token(0), Ready::readable());
    wait_for_hup!(poll);

    let mut buf = [0; 1024];
    let n = client.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn test_abrupt_shutdown() {
    use net2::TcpStreamExt;
    use std::io::{self, Read, Write};

    let mut poll = TestPoll::new();
    let mut buf = [0; 1024];

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let mut client = TcpStream::connect(&addr).unwrap();
    poll.register(&client,
                  Token(0),
                  Ready::readable() | Ready::writable(),
                  PollOpt::edge());

    let (mut socket, _) = listener.accept().unwrap();
    socket.set_linger(None);

    // Wait to be connected
    poll.wait_for(Token(0), Ready::writable());

    // Write some data

    client.write(b"junk").unwrap();

    socket.read(&mut buf[..1]).unwrap();

    drop(socket);

    poll.wait_for(Token(0), Ready::readable());
    poll.wait_for(Token(0), Ready::writable());
    wait_for_hup!(poll);

    let res = client.read(&mut buf);
    assert!(res.is_err(), "res = {:?}", res);
}
