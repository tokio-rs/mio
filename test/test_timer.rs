use mio::*;
use mio::tcp::*;
use bytes::{Buf, ByteBuf, SliceBuf};
use localhost;

use self::TestState::{Initial, AfterRead, AfterHup};

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);
const CONN: Token = Token(2);

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

    fn handle_read(&mut self, event_loop: &mut EventLoop<TestHandler>,
                   tok: Token, events: EventSet) {
        match tok {
            SERVER => {
                debug!("server connection ready for accept");
                let conn = self.srv.accept().unwrap().unwrap().0;
                event_loop.register(&conn, CONN, EventSet::all(),
                                        PollOpt::edge()).unwrap();
                event_loop.timeout_ms(conn, 200).unwrap();

                event_loop.reregister(&self.srv, SERVER, EventSet::readable(),
                                      PollOpt::edge()).unwrap();
            }
            CLIENT => {
                debug!("client readable");

                match self.state {
                    Initial => {
                        // Whether or not Hup is included with actual data is
                        // platform specific
                        if events.is_hup() {
                            self.state = AfterHup;
                        } else {
                            self.state = AfterRead;
                        }
                    }
                    AfterRead => {
                        assert_eq!(events, EventSet::readable() | EventSet::hup());
                        self.state = AfterHup;
                    }
                    AfterHup => panic!("Shouldn't get here"),
                }

                if self.state == AfterHup {
                    event_loop.shutdown();
                    return;
                }

                let mut buf = ByteBuf::mut_with_capacity(2048);

                match self.cli.try_read_buf(&mut buf) {
                    Ok(n) => {
                        debug!("read {:?} bytes", n);
                        assert!(b"zomg" == buf.flip().bytes());
                    }
                    Err(e) => {
                        debug!("client sock failed to read; err={:?}", e.kind());
                    }
                }

                event_loop.reregister(&self.cli, CLIENT,
                                      EventSet::readable() | EventSet::hup(),
                                      PollOpt::edge()).unwrap();
            }
            CONN => {}
            _ => panic!("received unknown token {:?}", tok),
        }
    }

    fn handle_write(&mut self, event_loop: &mut EventLoop<TestHandler>,
                    tok: Token, _: EventSet) {
        match tok {
            SERVER => panic!("received writable for token 0"),
            CLIENT => debug!("client connected"),
            CONN => {}
            _ => panic!("received unknown token {:?}", tok),
        }

        event_loop.reregister(&self.cli, CLIENT, EventSet::readable(),
                              PollOpt::edge()).unwrap();
    }
}

impl Handler for TestHandler {
    type Timeout = TcpStream;
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token, events: EventSet) {
        if events.is_readable() {
            self.handle_read(event_loop, tok, events);
        }

        if events.is_writable() {
            self.handle_write(event_loop, tok, events);
        }
    }

    fn timeout(&mut self, _event_loop: &mut EventLoop<TestHandler>, mut sock: TcpStream) {
        debug!("timeout handler : writing to socket");
        sock.try_write_buf(&mut SliceBuf::wrap(b"zomg")).unwrap().unwrap();
    }
}

#[test]
pub fn test_timer() {
    debug!("Starting TEST_TIMER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();

    let srv = TcpListener::bind(&addr).unwrap();

    info!("listening for connections");

    event_loop.register(&srv, SERVER, EventSet::all(), PollOpt::edge()).unwrap();

    let sock = TcpStream::connect(&addr).unwrap();

    // Connect to the server
    event_loop.register(&sock, CLIENT, EventSet::all(), PollOpt::edge()).unwrap();

    // Init the handler
    let mut handler = TestHandler::new(srv, sock);
    // Start the event loop
    event_loop.run(&mut handler).unwrap();

    assert!(handler.state == AfterHup, "actual={:?}", handler.state);
}
