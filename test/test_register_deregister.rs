use bytes::SliceBuf;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interests, Poll, PollOpt, Token};
use std::time::Duration;
use {localhost, TryWrite};

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
                sock.try_write_buf(&mut SliceBuf::wrap(b"foobar")).unwrap();
            }
            CLIENT => {
                trace!("handle_read; token=CLIENT");
                assert!(self.state == 0, "unexpected state {}", self.state);
                self.state = 1;
                poll.reregister(
                    &self.client,
                    CLIENT,
                    Interests::writable(),
                    PollOpt::level(),
                )
                .unwrap();
            }
            _ => panic!("unexpected token"),
        }
    }

    fn handle_write(&mut self, poll: &mut Poll, token: Token) {
        debug!("handle_write; token={:?}; state={:?}", token, self.state);

        assert!(token == CLIENT, "unexpected token {:?}", token);
        assert!(self.state == 1, "unexpected state {}", self.state);

        self.state = 2;
        poll.deregister(&self.client).unwrap();
        poll.deregister(&self.server).unwrap();
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
    poll.register(&server, SERVER, Interests::readable(), PollOpt::edge())
        .unwrap();

    let client = TcpStream::connect(&addr).unwrap();

    // Register client socket only as writable
    poll.register(&client, CLIENT, Interests::readable(), PollOpt::level())
        .unwrap();

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

    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
    assert!(events.iter().next().is_none());
}
