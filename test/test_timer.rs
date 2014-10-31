use mio::*;
use mio::net::*;
use mio::net::tcp::*;
use super::localhost;

type TestEventLoop = EventLoop<TcpSocket, ()>;

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

#[deriving(Show, PartialEq)]
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
    fn readable(&mut self, event_loop: &mut TestEventLoop, tok: Token, hint: ReadHint) {
        match tok {
            SERVER => {
                debug!("server connection ready for accept");
                let conn = self.srv.accept().unwrap().unwrap();
                event_loop.timeout_ms(conn, 200).unwrap();
            }
            CLIENT => {
                debug!("client readable");

                match self.state {
                    Initial => {
                        assert!(hint.contains(DATAHINT), "unexpected hint {}", hint);

                        // Whether or not Hup is included with actual data is platform specific
                        if hint.contains(HUPHINT) {
                            self.state = AfterHup;
                        } else {
                            self.state = AfterRead;
                        }
                    },
                    AfterRead => {
                        assert_eq!(hint, DATAHINT | HUPHINT);
                        self.state = AfterHup;
                    },
                    AfterHup => panic!("Shouldn't get here")
                }

                if self.state == AfterHup {
                    event_loop.shutdown();
                    return;
                }

                let mut buf = buf::ByteBuf::new(1024);

                match self.cli.read(&mut buf) {
                    Ok(_) => {
                        buf.flip();
                        assert!(b"zomg" == buf.bytes());
                    }
                    Err(e) => panic!("client sock failed to read; err={}", e),
                }
            }
            _ => panic!("received unknown token {}", tok)
        }
    }

    fn writable(&mut self, _event_loop: &mut TestEventLoop, tok: Token) {
        match tok {
            SERVER => panic!("received writable for token 0"),
            CLIENT => debug!("client connected"),
            _ => panic!("received unknown token {}", tok)
        }
    }

    fn timeout(&mut self, _event_loop: &mut TestEventLoop, mut sock: TcpSocket) {
        sock.write(&mut buf::wrap(b"zomg"))
            .unwrap().unwrap();
    }
}

#[test]
pub fn test_timer() {
    let mut event_loop = EventLoop::new().unwrap();

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();

    let srv = srv.bind(&addr).unwrap()
        .listen(256u).unwrap();

    info!("listening for connections");
    event_loop.register(&srv, SERVER).unwrap();

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    event_loop.connect(&sock, &addr, CLIENT).unwrap();

    // Start the event loop
    let handler = event_loop.run(TestHandler::new(srv, sock))
        .ok().expect("failed to execute event loop");

    assert!(handler.state == AfterHup, "actual={}", handler.state);
}
