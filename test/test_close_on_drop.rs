use mio::*;
use mio::buf::ByteBuf;
use super::localhost;

type TestEventLoop = EventLoop<uint, ()>;

struct TestHandler {
    srv: TcpAcceptor,
    cli: TcpSocket
}

impl TestHandler {
    fn new(srv: TcpAcceptor, cli: TcpSocket) -> TestHandler {
        TestHandler {
            srv: srv,
            cli: cli
        }
    }
}

impl Handler<uint, ()> for TestHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, tok: Token) {
        match tok {
            Token(0) => {
                debug!("server connection ready for accept");
                let _ = self.srv.accept().unwrap().unwrap();
            }
            Token(1) => {
                debug!("client readable");
                let mut buf = ByteBuf::new(1024);
                match self.cli.read(&mut buf) {
                    Err(e) if e.is_eof() => event_loop.shutdown(),
                    _ => fail!("the client socket should not be readable")
                }
            }
            _ => fail!("received unknown token {}", tok)
        }
    }

    fn writable(&mut self, _event_loop: &mut TestEventLoop, tok: Token) {
        match tok {
            Token(0) => fail!("received writable for token 0"),
            Token(1) => {
                debug!("client connected");
            }
            _ => fail!("received unknown token {}", tok)
        }
    }
}

#[test]
pub fn test_close_on_drop() {
    let mut event_loop = EventLoop::new().unwrap();

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();

    let srv = srv.bind(&addr).unwrap();

    info!("listening for connections");
    event_loop.listen(&srv, 256u, Token(0)).unwrap();

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    event_loop.connect(&sock, &addr, Token(1)).unwrap();

    // Start the event loop
    event_loop.run(TestHandler::new(srv, sock))
        .ok().expect("failed to execute event loop");
}
