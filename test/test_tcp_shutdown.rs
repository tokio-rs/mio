use std::collections::HashMap;
use std::net::Shutdown;
use std::time::{Duration, Instant};

use mio::{Token, Ready, PollOpt, Poll, Events};
use mio::event::{Evented, Event};
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

    fn wait_for(&mut self, token: Token, ready: Ready) -> Result<(), &'static str> {
        let now = Instant::now();

        loop {
            if now.elapsed() > Duration::from_secs(5) {
                return Err("not ready");
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
        Ok(())
    }

    fn check_idle(&mut self) -> Result<(), Event> {
        self.poll.poll(&mut self.events, Some(Duration::from_millis(100))).unwrap();

        if let Some(e) = self.events.iter().next() {
            Err(e)
        } else {
            Ok(())
        }
    }

    fn readiness(&self, token: Token) -> Ready {
        self.buf.get(&token).map(|r| *r).unwrap_or(Ready::empty())
    }
}

macro_rules! wait_for {
    ($poll:expr, $token:expr, $ready:expr) => {{
        match $poll.wait_for($token, $ready) {
            Ok(_) => {}
            Err(_) => panic!("not ready; token = {:?}; interest = {:?}", $token, $ready),
        }
    }}
}

macro_rules! wait_for_hup {
    ($poll:expr) => {
        #[cfg(unix)]
        {
            use mio::unix::UnixReady;
            wait_for!($poll, Token(0), Ready::from(UnixReady::hup()))
        }
    }
}

macro_rules! assert_idle {
    ($poll:expr) => {
        match $poll.check_idle() {
            Ok(()) => {}
            Err(e) => panic!("not idle; event = {:?}", e),
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

    wait_for!(poll, Token(0), Ready::writable());

    // Polling should not have any events
    assert_idle!(poll);

    // Now, shutdown the write half of the socket.
    socket.shutdown(Shutdown::Write).unwrap();

    wait_for!(poll, Token(0), Ready::readable());

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

    wait_for!(poll, Token(0), Ready::writable());

    // Polling should not have any events
    assert_idle!(poll);

    // Now, shutdown the write half of the socket.
    client.shutdown(Shutdown::Write).unwrap();

    let n = socket.read(&mut buf).unwrap();
    assert_eq!(0, n);
    drop(socket);

    wait_for!(poll, Token(0), Ready::readable());
    wait_for_hup!(poll);

    let mut buf = [0; 1024];
    let n = client.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn test_abrupt_shutdown() {
    use net2::TcpStreamExt;
    use std::io::{Read, Write};

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
    socket.set_linger(None).unwrap();

    // Wait to be connected
    poll.wait_for(Token(0), Ready::writable());

    // Write some data

    client.write(b"junk").unwrap();

    socket.read(&mut buf[..1]).unwrap();

    drop(socket);

    wait_for!(poll, Token(0), Ready::readable());
    wait_for!(poll, Token(0), Ready::writable());

    let res = client.read(&mut buf);
    assert!(res.is_err(), "res = {:?}", res);
}
