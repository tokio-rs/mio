use mio::*;
use mio::buf::ByteBuf;
use mio::net::*;
use mio::net::tcp::*;
use super::localhost;

use self::TestState::{Initial, AfterRead, AfterHup};

type TestEventLoop = EventLoop<usize, ()>;


#[derive(Debug, PartialEq)]
enum TestState {
    Initial,
    AfterRead,
    AfterHup
}

struct TestHandler {
    srv: TcpAcceptor,
    cli: TcpSocket,
    state: TestState
}

impl TestHandler {
    fn new(srv: TcpAcceptor, cli: TcpSocket) -> TestHandler {
        TestHandler {
            srv: srv,
            cli: cli,
            state: Initial
        }
    }
}

impl Handler<usize, ()> for TestHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, tok: Token, hint: ReadHint) {
        debug!("readable; tok={:?}; hint={:?}", tok, hint);

        match tok {
            Token(0) => {
                debug!("server connection ready for accept");
                let _ = self.srv.accept().unwrap().unwrap();
            }
            Token(1) => {
                debug!("client readable");

                match self.state {
                    Initial => {
                        assert!(hint.is_data(), "unexpected hint {:?}", hint);

                        // Whether or not Hup is included with actual data is platform specific
                        if hint.is_hup() {
                            self.state = AfterHup;
                        } else {
                            self.state = AfterRead;
                        }
                    },
                    AfterRead => {
                        //assert_eq!(hint, DATAHINT | HUPHINT);
                        self.state = AfterHup;
                    },
                    AfterHup => panic!("Shouldn't get here")
                }

                let mut buf = ByteBuf::mut_with_capacity(1024);

                match self.cli.read(&mut buf) {
                    Err(e) if e.is_eof() => event_loop.shutdown(),
                    _ => panic!("the client socket should not be readable")
                }
            }
            _ => panic!("received unknown token {:?}", tok)
        }
        event_loop.reregister(&self.cli, Token(1), Interest::readable() | Interest::hup(), PollOpt::edge()).unwrap();
    }

    fn writable(&mut self, _event_loop: &mut TestEventLoop, tok: Token) {
        match tok {
            Token(0) => panic!("received writable for token 0"),
            Token(1) => {
                debug!("client connected");
                _event_loop.reregister(&self.cli, Token(1), Interest::readable() | Interest::hup(), PollOpt::edge()).unwrap();
            }
            _ => panic!("received unknown token {:?}", tok)
        }
    }
}

#[test]
pub fn test_close_on_drop() {
    debug!("Starting TEST_CLOSE_ON_DROP");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();

    let sock = TcpSocket::v4().unwrap();

    event_loop.register_opt(&sock, Token(1), Interest::writable(), PollOpt::edge()).unwrap();

    let srv = srv.bind(&addr).unwrap().listen(256).unwrap();

    info!("register server socket");
    event_loop.register_opt(&srv, Token(0), Interest::readable(), PollOpt::edge()).unwrap();
    // Connect to the server
    sock.connect(&addr).unwrap();

    let mut handler = TestHandler::new(srv, sock);

    // Start the event loop
    event_loop.run(&mut handler).unwrap();

    assert!(handler.state == AfterHup, "actual={:?}", handler.state);
}
