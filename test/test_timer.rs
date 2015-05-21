use mio::*;
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
    type Timeout = TcpStream;
    type Message = ();

    fn readable(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token, hint: ReadHint) {
        match tok {
            SERVER => {
                debug!("server connection ready for accept");
                let conn = self.srv.accept().unwrap().unwrap();
                event_loop.timeout_ms(conn, 200).unwrap();

                event_loop.reregister(&self.srv, SERVER, Interest::readable(), PollOpt::edge()).unwrap();
            }
            CLIENT => {
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
                    }
                    AfterRead => {
                        assert_eq!(hint, ReadHint::data() | ReadHint::hup());
                        self.state = AfterHup;
                    }
                    AfterHup => panic!("Shouldn't get here"),
                }

                if self.state == AfterHup {
                    event_loop.shutdown();
                    return;
                }

                let mut buf = buf::ByteBuf::mut_with_capacity(2048);

                match self.cli.try_read_buf(&mut buf) {
                    Ok(n) => {
                        debug!("read {:?} bytes", n);
                        assert!(b"zomg" == buf.flip().bytes());
                    }
                    Err(e) => {
                        debug!("client sock failed to read; err={:?}", e.kind());
                    }
                }

                event_loop.reregister(&self.cli, CLIENT, Interest::readable() | Interest::hup(), PollOpt::edge()).unwrap();
            }
            _ => panic!("received unknown token {:?}", tok),
        }
    }

    fn writable(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token) {
        match tok {
            SERVER => panic!("received writable for token 0"),
            CLIENT => debug!("client connected"),
            _ => panic!("received unknown token {:?}", tok),
        }

        event_loop.reregister(&self.cli, CLIENT, Interest::readable(), PollOpt::edge()).unwrap();
    }

    fn timeout(&mut self, _event_loop: &mut EventLoop<TestHandler>, mut sock: TcpStream) {
        debug!("timeout handler : writing to socket");
        sock.try_write_buf(&mut buf::SliceBuf::wrap(b"zomg")).unwrap().unwrap();
    }
}

#[test]
pub fn test_timer() {
    debug!("Starting TEST_TIMER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();
    srv.bind(&addr).unwrap();

    let srv = srv.listen(256).unwrap();

    info!("listening for connections");

    event_loop.register_opt(&srv, SERVER, Interest::all(), PollOpt::edge()).unwrap();

    let (sock, _) = TcpSocket::v4().unwrap()
        .connect(&addr).unwrap();

    // Connect to the server
    event_loop.register_opt(&sock, CLIENT, Interest::all(), PollOpt::edge()).unwrap();

    // Init the handler
    let mut handler = TestHandler::new(srv, sock);
    // Start the event loop
    event_loop.run(&mut handler).unwrap();

    assert!(handler.state == AfterHup, "actual={:?}", handler.state);
}
