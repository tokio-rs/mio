use {localhost, TryRead};
use mio::*;
use mio::deprecated::{EventLoop, Handler};
use bytes::ByteBuf;
use mio::tcp::*;

use self::TestState::{Initial, AfterRead};

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
    state: TestState
}

impl TestHandler {
    fn new(srv: TcpListener, cli: TcpStream) -> TestHandler {
        TestHandler {
            srv: srv,
            cli: cli,
            state: Initial
        }
    }

    fn handle_read(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token, events: Ready) {
        debug!("readable; tok={:?}; hint={:?}", tok, events);

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
                    },
                    AfterRead => {}
                }

                let mut buf = ByteBuf::mut_with_capacity(1024);

                match self.cli.try_read_buf(&mut buf) {
                    Ok(Some(0)) => event_loop.shutdown(),
                    _ => panic!("the client socket should not be readable")
                }
            }
            _ => panic!("received unknown token {:?}", tok)
        }
        event_loop.reregister(&self.cli, CLIENT, Ready::readable() | Ready::hup(), PollOpt::edge()).unwrap();
    }

    fn handle_write(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token, _: Ready) {
        match tok {
            SERVER => panic!("received writable for token 0"),
            CLIENT => {
                debug!("client connected");
                event_loop.reregister(&self.cli, CLIENT, Ready::readable() | Ready::hup(), PollOpt::edge()).unwrap();
            }
            _ => panic!("received unknown token {:?}", tok)
        }
    }
}


impl Handler for TestHandler {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token, events: Ready) {
        if events.is_readable() {
            self.handle_read(event_loop, tok, events);
        }

        if events.is_writable() {
            self.handle_write(event_loop, tok, events);
        }
    }
}

#[test]
pub fn test_close_on_drop() {
    debug!("Starting TEST_CLOSE_ON_DROP");
    let mut event_loop = EventLoop::new().unwrap();

    // The address to connect to - localhost + a unique port
    let addr = localhost();

    // == Create & setup server socket
    let srv = TcpListener::bind(&addr).unwrap();

    event_loop.register(&srv, SERVER, Ready::readable(), PollOpt::edge()).unwrap();

    // == Create & setup client socket
    let sock = TcpStream::connect(&addr).unwrap();

    event_loop.register(&sock, CLIENT, Ready::writable(), PollOpt::edge()).unwrap();

    // == Setup test handler
    let mut handler = TestHandler::new(srv, sock);

    // == Run test
    event_loop.run(&mut handler).unwrap();
    assert!(handler.state == AfterRead, "actual={:?}", handler.state);
}
