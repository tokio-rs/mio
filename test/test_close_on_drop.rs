use mio::*;
use mio::buf::ByteBuf;
use mio::tcp::*;
use super::localhost;

use self::TestState::{Initial, AfterRead, AfterHup};

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

#[derive(Debug, PartialEq)]
enum TestState {
    Initial,
    AfterRead,
    AfterHup
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
}

impl Handler for TestHandler {
    type Timeout = ();
    type Message = ();

    fn readable(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token, hint: ReadHint) {
        debug!("readable; tok={:?}; hint={:?}", tok, hint);

        match tok {
            SERVER => {
                debug!("server connection ready for accept");
                let _ = self.srv.accept().unwrap().unwrap();
            }
            CLIENT => {
                debug!("client readable");

                match self.state {
                    Initial => {
                        let mut buf = [0; 4096];
                        debug!("GOT={:?}", self.cli.try_read(&mut buf[..]));
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

                match self.cli.try_read_buf(&mut buf) {
                    Ok(Some(0)) => event_loop.shutdown(),
                    _ => panic!("the client socket should not be readable")
                }
            }
            _ => panic!("received unknown token {:?}", tok)
        }
        event_loop.reregister(&self.cli, CLIENT, Interest::readable() | Interest::hup(), PollOpt::edge()).unwrap();
    }

    fn writable(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token) {
        match tok {
            SERVER => panic!("received writable for token 0"),
            CLIENT => {
                debug!("client connected");
                event_loop.reregister(&self.cli, CLIENT, Interest::readable() | Interest::hup(), PollOpt::edge()).unwrap();
            }
            _ => panic!("received unknown token {:?}", tok)
        }
    }
}

#[test]
pub fn test_close_on_drop() {
    ::env_logger::init().unwrap();

    debug!("Starting TEST_CLOSE_ON_DROP");
    let mut event_loop = EventLoop::new().unwrap();

    // The address to connect to - localhost + a unique port
    let addr = localhost();

    // == Create & setup server socket
    let srv = TcpSocket::v4().unwrap();
    srv.set_reuseaddr(true).unwrap();
    srv.bind(&addr).unwrap();

    let srv = srv.listen(256).unwrap();

    event_loop.register_opt(&srv, SERVER, Interest::readable(), PollOpt::edge()).unwrap();

    // == Create & setup client socket
    let (sock, _) = TcpSocket::v4().unwrap()
        .connect(&addr).unwrap();

    event_loop.register_opt(&sock, CLIENT, Interest::writable(), PollOpt::edge()).unwrap();

    // == Setup test handler
    let mut handler = TestHandler::new(srv, sock);

    // == Run test
    event_loop.run(&mut handler).unwrap();
    assert!(handler.state == AfterHup, "actual={:?}", handler.state);
}
