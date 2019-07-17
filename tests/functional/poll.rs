use std::io::{self, Write};
use std::thread::sleep;
use std::time::Duration;

use log::{debug, info, trace};

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interests, Poll, Registry, Token};

use crate::util::localhost;

#[test]
fn run_once_with_nothing() {
    let mut events = Events::with_capacity(1024);
    let mut poll = Poll::new().unwrap();
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
}

#[test]
fn test_poll_closes_fd() {
    for _ in 0..2000 {
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(4);

        poll.poll(&mut events, Some(Duration::from_millis(0)))
            .unwrap();

        drop(poll);
    }
}

#[test]
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn double_register() {
    let poll = Poll::new().unwrap();

    let l = TcpListener::bind(localhost()).unwrap();

    poll.registry()
        .register(&l, Token(0), Interests::READABLE)
        .unwrap();

    assert!(poll
        .registry()
        .register(&l, Token(1), Interests::READABLE)
        .is_err());
}

#[test]
fn register_and_drop() {
    let mut events = Events::with_capacity(1024);
    let mut poll = Poll::new().unwrap();

    let l = TcpListener::bind(localhost()).unwrap();
    poll.registry()
        .register(&l, Token(1), Interests::READABLE | Interests::WRITABLE)
        .unwrap();
    drop(l);
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
}

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

struct TestHandler {
    server: TcpListener,
    client: TcpStream,
    state: usize,
}

impl TestHandler {
    fn new(srv: TcpListener, cli: TcpStream) -> TestHandler {
        TestHandler {
            server: srv,
            client: cli,
            state: 0,
        }
    }

    fn handle_read(&mut self, registry: &Registry, token: Token) {
        match token {
            SERVER => {
                trace!("handle_read; token=SERVER");
                let mut sock = self.server.accept().unwrap().0;
                if let Err(err) = sock.write(b"foobar") {
                    if err.kind() != io::ErrorKind::WouldBlock {
                        panic!("unexpected error writing to connection: {}", err);
                    }
                }
            }
            CLIENT => {
                trace!("handle_read; token=CLIENT");
                assert!(self.state == 0, "unexpected state {}", self.state);
                self.state = 1;
                registry
                    .reregister(&self.client, CLIENT, Interests::WRITABLE)
                    .unwrap();
            }
            _ => panic!("unexpected token"),
        }
    }

    fn handle_write(&mut self, registry: &Registry, token: Token) {
        debug!("handle_write; token={:?}; state={:?}", token, self.state);

        assert!(token == CLIENT, "unexpected token {:?}", token);
        assert!(self.state == 1, "unexpected state {}", self.state);

        self.state = 2;
        registry.deregister(&self.client).unwrap();
        registry.deregister(&self.server).unwrap();
    }
}

#[test]
fn register_deregister() {
    drop(env_logger::try_init());

    debug!("Starting TEST_REGISTER_DEREGISTER");
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(1024);

    let server = TcpListener::bind(localhost()).unwrap();

    info!("register server socket");
    poll.registry()
        .register(&server, SERVER, Interests::READABLE)
        .unwrap();

    let client = TcpStream::connect(server.local_addr().unwrap()).unwrap();

    // Register client socket only as readable.
    poll.registry()
        .register(&client, CLIENT, Interests::READABLE)
        .unwrap();

    let mut handler = TestHandler::new(server, client);

    loop {
        poll.poll(&mut events, None).unwrap();

        if let Some(event) = events.iter().next() {
            if event.is_readable() {
                handler.handle_read(poll.registry(), event.token());
            }

            if event.is_writable() {
                handler.handle_write(poll.registry(), event.token());
                break;
            }
        }
    }

    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
    assert!(events.iter().next().is_none());
}

#[test]
fn reregister() {
    let mut events = Events::with_capacity(1024);
    let mut poll = Poll::new().unwrap();

    let l = TcpListener::bind(localhost()).unwrap();

    poll.registry()
        .register(&l, Token(0), Interests::READABLE)
        .unwrap();

    let s1 = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    poll.registry()
        .register(&s1, Token(2), Interests::READABLE)
        .unwrap();

    const TIMEOUT: Duration = Duration::from_millis(1000);
    sleep(TIMEOUT);

    // FIXME: writable interests make no sense for TcpListener.
    poll.registry()
        .reregister(&l, Token(0), Interests::WRITABLE)
        .unwrap();

    poll.poll(&mut events, Some(TIMEOUT)).unwrap();
    assert!(events.is_empty());
}
