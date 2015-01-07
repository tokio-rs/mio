use mio::*;
use mio::net::*;
use mio::net::tcp::*;
use super::localhost;
use mio::event as evt;
use std::time::Duration;

use self::TestState::{Initial, AfterRead, AfterHup};

type TestEventLoop = EventLoop<TcpSocket, ()>;

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

#[derive(Show, PartialEq)]
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

impl Handler<TcpSocket, ()> for TestHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, tok: Token, hint: evt::ReadHint) {
        match tok {
            SERVER => {
                debug!("server connection ready for accept");
                let conn = self.srv.accept().unwrap().unwrap();
                event_loop.timeout(conn, Duration::milliseconds(200)).unwrap();

                event_loop.reregister(&self.srv, SERVER, evt::READABLE, evt::EDGE).unwrap();
            }
            CLIENT => {
                debug!("client readable");

                match self.state {
                    Initial => {
                        assert!(hint.contains(evt::DATAHINT), "unexpected hint {:?}", hint);

                        // Whether or not Hup is included with actual data is platform specific
                        if hint.contains(evt::HUPHINT) {
                            self.state = AfterHup;
                        } else {
                            self.state = AfterRead;
                        }
                    }
                    AfterRead => {
                        assert_eq!(hint, evt::DATAHINT | evt::HUPHINT);
                        self.state = AfterHup;
                    }
                    AfterHup => panic!("Shouldn't get here"),
                }

                if self.state == AfterHup {
                    event_loop.shutdown();
                    return;
                }

                let mut buf = buf::ByteBuf::new(2048);

                match self.cli.read(&mut buf) {
                    Ok(n) => {
                        debug!("read {:?} bytes", n);
                        buf.flip();
                        assert!(b"zomg" == buf.bytes());
                    }
                    Err(e) => {
                        debug!("client sock failed to read; err={:?}", e.kind);
                    }
                }

                event_loop.reregister(&self.cli, CLIENT, evt::READABLE | evt::HUP, evt::EDGE).unwrap();
            }
            _ => panic!("received unknown token {:?}", tok),
        }
    }

    fn writable(&mut self, event_loop: &mut TestEventLoop, tok: Token) {
        match tok {
            SERVER => panic!("received writable for token 0"),
            CLIENT => debug!("client connected"),
            _ => panic!("received unknown token {:?}", tok),
        }

        event_loop.reregister(&self.cli, CLIENT, evt::READABLE, evt::EDGE).unwrap();
    }

    fn timeout(&mut self, _event_loop: &mut TestEventLoop, sock: TcpSocket) {
        debug!("timeout handler : writing to socket");
        sock.write(&mut buf::wrap(b"zomg")).unwrap().unwrap();
    }
}

#[test]
pub fn test_timer() {
    debug!("Starting TEST_TIMER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();

    let srv = srv.bind(&addr).unwrap().listen(256u).unwrap();

    info!("listening for connections");

    event_loop.register_opt(&srv, SERVER, evt::ALL, evt::EDGE).unwrap();

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    event_loop.register_opt(&sock, CLIENT, evt::ALL, evt::EDGE).unwrap();
    sock.connect(&addr).unwrap();
    // Start the event loop
    let handler = event_loop.run(TestHandler::new(srv, sock))
        .ok().expect("failed to execute event loop");

    assert!(handler.state == AfterHup, "actual={:?}", handler.state);
}
