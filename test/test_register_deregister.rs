use bytes::SliceBuf;
use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Poll, PollOpt, Ready, Registry, Token};
use std::time::Duration;
use {expect_events, localhost, TryWrite};

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
                sock.try_write_buf(&mut SliceBuf::wrap(b"foobar")).unwrap();
            }
            CLIENT => {
                trace!("handle_read; token=CLIENT");
                assert!(self.state == 0, "unexpected state {}", self.state);
                self.state = 1;
                registry
                    .reregister(&self.client, CLIENT, Ready::writable(), PollOpt::level())
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
pub fn test_register_deregister() {
    let _ = ::env_logger::init();

    debug!("Starting TEST_REGISTER_DEREGISTER");
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(1024);

    let addr = localhost();

    let server = TcpListener::bind(&addr).unwrap();

    info!("register server socket");
    poll.registry()
        .register(&server, SERVER, Ready::readable(), PollOpt::edge())
        .unwrap();

    let client = TcpStream::connect(&addr).unwrap();

    // Register client socket only as writable
    poll.registry()
        .register(&client, CLIENT, Ready::readable(), PollOpt::level())
        .unwrap();

    let mut handler = TestHandler::new(server, client);

    loop {
        poll.poll(&mut events, None).unwrap();

        if let Some(event) = events.iter().next() {
            if event.readiness().is_readable() {
                handler.handle_read(poll.registry(), event.token());
            }

            if event.readiness().is_writable() {
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
pub fn test_register_empty_interest() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(1024);
    let addr = localhost();

    let sock = TcpListener::bind(&addr).unwrap();

    poll.registry()
        .register(&sock, Token(0), Ready::empty(), PollOpt::edge())
        .unwrap();

    let client = TcpStream::connect(&addr).unwrap();

    // The connect is not guaranteed to have started until it is registered
    // https://docs.rs/mio/0.6.10/mio/struct.Poll.html#registering-handles
    poll.registry()
        .register(&client, Token(1), Ready::empty(), PollOpt::edge())
        .unwrap();

    // sock is registered with empty interest, we should not receive any event
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
    assert!(
        events.iter().next().is_none(),
        "Received unexpected event: {:?}",
        events.iter().next().unwrap()
    );

    // now sock is reregistered with readable, we should receive the pending event
    poll.registry()
        .reregister(&sock, Token(0), Ready::readable(), PollOpt::edge())
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        2,
        vec![Event::new(Ready::readable(), Token(0))],
    );

    poll.registry()
        .reregister(&sock, Token(0), Ready::empty(), PollOpt::edge())
        .unwrap();
}
