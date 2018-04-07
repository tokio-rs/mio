use {expect_events, localhost, TryWrite};
use mio::{Events, Poll, PollOpt, Ready, Token};
use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use bytes::IntoBuf;
use std::time::Duration;

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

    fn handle_read(&mut self, poll: &mut Poll, token: Token) {
        match token {
            SERVER => {
                trace!("handle_read; token=SERVER");
                let mut sock = self.server.accept().unwrap().0;
                sock.try_write_buf(&mut b"foobar".into_buf()).unwrap();
            }
            CLIENT => {
                trace!("handle_read; token=CLIENT");
                assert!(self.state == 0, "unexpected state {}", self.state);
                self.state = 1;
                poll.register().reregister(&self.client, CLIENT, Ready::writable(), PollOpt::LEVEL).unwrap();
            }
            _ => panic!("unexpected token"),
        }
    }

    fn handle_write(&mut self, poll: &mut Poll, token: Token) {
        debug!("handle_write; token={:?}; state={:?}", token, self.state);

        assert!(token == CLIENT, "unexpected token {:?}", token);
        assert!(self.state == 1, "unexpected state {}", self.state);

        self.state = 2;
        poll.register().deregister(&self.client).unwrap();
        poll.register().deregister(&self.server).unwrap();
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
    poll.register().register(&server, SERVER, Ready::readable(), PollOpt::EDGE).unwrap();

    let client = TcpStream::connect(&addr).unwrap();

    // Register client socket only as writable
    poll.register().register(&client, CLIENT, Ready::readable(), PollOpt::LEVEL).unwrap();

    let mut handler = TestHandler::new(server, client);

    loop {
        poll.poll(&mut events, None).unwrap();

        if let Some(event) = events.iter().next() {
            if event.readiness().is_readable() {
                handler.handle_read(&mut poll, event.token());
            }

            if event.readiness().is_writable() {
                handler.handle_write(&mut poll, event.token());
                break;
            }
        }
    }

    poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
    assert!(events.iter().next().is_none());
}

#[test]
pub fn test_register_empty_interest() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(1024);
    let addr = localhost();

    let sock = TcpListener::bind(&addr).unwrap();

    poll.register().register(&sock, Token(0), Ready::empty(), PollOpt::EDGE).unwrap();

    let client = TcpStream::connect(&addr).unwrap();

    // The connect is not guaranteed to have started until it is registered
    // https://docs.rs/mio/0.6.10/mio/struct.Poll.html#registering-handles
    poll.register().register(&client, Token(1), Ready::empty(), PollOpt::EDGE).unwrap();

    // sock is registered with empty interest, we should not receive any event
    poll.poll(&mut events, Some(Duration::from_millis(100))).unwrap();
    assert!(events.iter().next().is_none(), "Received unexpected event: {:?}", events.iter().next().unwrap());

    // now sock is reregistered with readable, we should receive the pending event
    poll.register().reregister(&sock, Token(0), Ready::readable(), PollOpt::EDGE).unwrap();
    expect_events(&mut poll, &mut events, 2, vec![
        Event::new(Ready::readable(), Token(0))
    ]);

    poll.register().reregister(&sock, Token(0), Ready::empty(), PollOpt::EDGE).unwrap();
}
