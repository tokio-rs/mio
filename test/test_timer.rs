use mio::*;
use mio::net::*;
use mio::net::tcp::*;
use super::localhost;
use mio::event_ctx as evt;

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

                let evt = IoEventCtx::new(evt::IOREADABLE | evt::IOEDGE, CLIENT); 
                event_loop.reregister(&self.srv, &evt).unwrap();
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
                
                let mut buf = buf::ByteBuf::new(2048);

                match self.cli.read(&mut buf) {
                    Ok(n) => {
                        debug!("read {} bytes", n);
                        buf.flip();
                        assert!(b"zomg" == buf.bytes());
                    }
                    Err(e) => match e.kind {
                      _ => { debug!("client sock failed to read; err={}", e.kind); }
                    }
                }
                
                let evt = IoEventCtx::new(evt::IOREADABLE | evt::IOHUPHINT | evt::IOEDGE, CLIENT); 
                event_loop.reregister(&self.cli, &evt).unwrap();
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
        let evt = IoEventCtx::new(evt::IOREADABLE | evt::IOHUPHINT | evt::IOEDGE, tok); 
        _event_loop.reregister(&self.cli, &evt).unwrap();
    }

    fn timeout(&mut self, _event_loop: &mut TestEventLoop, mut sock: TcpSocket) {
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

    let srv = srv.bind(&addr).unwrap()
        .listen(256u).unwrap();

    info!("listening for connections");
    let evt = IoEventCtx::new(evt::IOALL | evt::IOEDGE, SERVER);
    event_loop.register(&srv, &evt).unwrap();

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    let evt = IoEventCtx::new(evt::IOALL | evt::IOHUPHINT | evt::IOEDGE, CLIENT); 
    event_loop.register(&sock, &evt).unwrap();
    sock.connect(&addr).unwrap();
    // Start the event loop
    let handler = event_loop.run(TestHandler::new(srv, sock))
        .ok().expect("failed to execute event loop");

    assert!(handler.state == AfterHup, "actual={}", handler.state);
}
