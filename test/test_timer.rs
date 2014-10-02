use mio::*;
use super::localhost;

type TestEventLoop = EventLoop<TcpSocket, ()>;

static SERVER: Token = TOKEN_0;
static CLIENT: Token = TOKEN_1;

struct TestHandler {
    srv: TcpAcceptor,
    cli: TcpSocket,
}

impl TestHandler {
    fn new(srv: TcpAcceptor, cli: TcpSocket) -> TestHandler {
        TestHandler {
            srv: srv,
            cli: cli
        }
    }
}

impl Handler<TcpSocket, ()> for TestHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, tok: Token) {
        match tok {
            SERVER => {
                debug!("server connection ready for accept");
                let conn = self.srv.accept().unwrap().unwrap();
                event_loop.timeout_ms(conn, 200).unwrap();
            }
            CLIENT => {
                debug!("client readable");
                let mut buf = buf::ByteBuf::new(1024);
                match self.cli.read(&mut buf) {
                    Ok(_) => {
                        buf.flip();
                        assert!(b"zomg" == buf.bytes());
                        event_loop.shutdown();
                    }
                    Err(e) => fail!("client sock failed to read; err={}", e),
                }
            }
            _ => fail!("received unknown token {}", tok)
        }
    }

    fn writable(&mut self, _event_loop: &mut TestEventLoop, tok: Token) {
        match tok {
            SERVER => fail!("received writable for token 0"),
            CLIENT => debug!("client connected"),
            _ => fail!("received unknown token {}", tok)
        }
    }

    fn timeout(&mut self, _event_loop: &mut TestEventLoop, mut sock: TcpSocket) {
        sock.write(&mut buf::wrap(b"zomg"))
            .unwrap().unwrap();
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
    event_loop.listen(&srv, 256u, SERVER).unwrap();

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    event_loop.connect(&sock, &addr, CLIENT).unwrap();

    // Start the event loop
    event_loop.run(TestHandler::new(srv, sock))
        .ok().expect("failed to execute event loop");
}
