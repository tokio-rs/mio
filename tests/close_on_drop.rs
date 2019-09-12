use bytes::BytesMut;
use log::debug;

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interests, Poll, Token};

mod util;

use util::{any_local_address, init, TryRead};

use self::TestState::{AfterRead, Initial};

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

#[derive(Debug, PartialEq)]
enum TestState {
    Initial,
    AfterRead,
}

struct TestHandler {
    srv: TcpListener,
    cli: TcpStream,
    state: TestState,
    shutdown: bool,
}

impl TestHandler {
    fn new(srv: TcpListener, cli: TcpStream) -> TestHandler {
        TestHandler {
            srv,
            cli,
            state: Initial,
            shutdown: false,
        }
    }

    fn handle_read(&mut self, poll: &mut Poll, tok: Token) {
        debug!("readable; tok={:?}", tok);

        match tok {
            SERVER => {
                debug!("server connection ready for accept");
                let _ = self.srv.accept().unwrap();
            }
            CLIENT => {
                debug!("client readable");

                match self.state {
                    Initial => {
                        let mut buf = [0; 4096];
                        debug!("GOT={:?}", self.cli.try_read(&mut buf[..]));
                        self.state = AfterRead;
                    }
                    AfterRead => {}
                }

                let mut buf = BytesMut::with_capacity(1024);

                match self.cli.try_read_buf(&mut buf) {
                    Ok(Some(0)) => self.shutdown = true,
                    Ok(_) => panic!("the client socket should not be readable"),
                    Err(e) => panic!("Unexpected error {:?}", e),
                }
            }
            _ => panic!("received unknown token {:?}", tok),
        }
        poll.registry()
            .reregister(&self.cli, CLIENT, Interests::READABLE)
            .unwrap();
    }

    fn handle_write(&mut self, poll: &mut Poll, tok: Token) {
        match tok {
            SERVER => panic!("received writable for token 0"),
            CLIENT => {
                debug!("client connected");
                poll.registry()
                    .reregister(&self.cli, CLIENT, Interests::READABLE)
                    .unwrap();
            }
            _ => panic!("received unknown token {:?}", tok),
        }
    }
}

#[test]
pub fn close_on_drop() {
    init();
    debug!("Starting TEST_CLOSE_ON_DROP");
    let mut poll = Poll::new().unwrap();

    // == Create & setup server socket
    let srv = TcpListener::bind(any_local_address()).unwrap();
    let addr = srv.local_addr().unwrap();

    poll.registry()
        .register(&srv, SERVER, Interests::READABLE)
        .unwrap();

    // == Create & setup client socket
    let sock = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&sock, CLIENT, Interests::WRITABLE)
        .unwrap();

    // == Create storage for events
    let mut events = Events::with_capacity(1024);

    // == Setup test handler
    let mut handler = TestHandler::new(srv, sock);

    // == Run test
    while !handler.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.is_readable() {
                handler.handle_read(&mut poll, event.token());
            }

            if event.is_writable() {
                handler.handle_write(&mut poll, event.token());
            }
        }
    }
    assert!(handler.state == AfterRead, "actual={:?}", handler.state);
}
