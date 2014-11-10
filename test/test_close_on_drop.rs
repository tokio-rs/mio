use mio::*;
use mio::buf::ByteBuf;
use mio::net::*;
use mio::net::tcp::*;
use super::localhost;
//use mio::event_ctx::IoEventCtx;
use mio::event_ctx as evt;

type TestEventLoop = EventLoop<uint, ()>;

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

impl Handler<uint, ()> for TestHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, tok: Token, hint: ReadHint) {
        debug!("readable; tok={}; hint={}", tok, hint);

        match tok {
            Token(0) => {
                debug!("server connection ready for accept");
                let _ = self.srv.accept().unwrap().unwrap();
            }
            Token(1) => {
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
                        //assert_eq!(hint, DATAHINT | HUPHINT);
                        self.state = AfterHup;
                    },
                    AfterHup => panic!("Shouldn't get here")
                }

                let mut buf = ByteBuf::new(1024);

                match self.cli.read(&mut buf) {
                    Err(e) if e.is_eof() => event_loop.shutdown(),
                    _ => panic!("the client socket should not be readable")
                }
            }
            _ => panic!("received unknown token {}", tok)
        }
        let cevt = IoEventCtx::new(evt::IOREADABLE | evt::IOEDGE | evt::IOHUPHINT, Token(1));
        event_loop.reregister(&self.cli, &cevt).unwrap();
    }

    fn writable(&mut self, _event_loop: &mut TestEventLoop, tok: Token) {
        match tok {
            Token(0) => panic!("received writable for token 0"),
            Token(1) => {
                debug!("client connected");
                let cevt = IoEventCtx::new(evt::IOREADABLE | evt::IOEDGE | evt::IOHUPHINT, Token(1));
                _event_loop.reregister(&self.cli, &cevt).unwrap();
            }
            _ => panic!("received unknown token {}", tok)
        }
    }
}

#[test]
pub fn test_close_on_drop() {
    debug!("Starting TEST_CLOSE_ON_DROP");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();
    
    let sock = TcpSocket::v4().unwrap();

    let cevt = IoEventCtx::new(evt::IOWRITABLE | evt::IOEDGE | evt::IOHUPHINT, Token(1));
    event_loop.register(&sock, &cevt).unwrap();

    let srv = srv.bind(&addr).unwrap().listen(256).unwrap();

    info!("register server socket");
    let evt = IoEventCtx::new(evt::IOREADABLE | evt::IOEDGE, Token(0));
    event_loop.register(&srv, &evt).unwrap();
    // Connect to the server
    sock.connect(&addr).unwrap();
    // Start the event loop
    let handler = event_loop.run(TestHandler::new(srv, sock)).ok().expect("failed to execute event loop");

    assert!(handler.state == AfterHup, "actual={}", handler.state);
}
