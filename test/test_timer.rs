use mio::*;
use super::localhost;

type TestReactor = Reactor<TcpSocket, ()>;

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
    fn readable(&mut self, reactor: &mut TestReactor, tok: Token) {
        match tok {
            SERVER => {
                debug!("server connection ready for accept");
                let conn = self.srv.accept().unwrap().unwrap();
                reactor.timeout_ms(conn, 200).unwrap();
            }
            CLIENT => {
                debug!("client readable");
                let mut buf = buf::ByteBuf::new(1024);
                match self.cli.read(&mut buf) {
                    Ok(_) => {
                        buf.flip();
                        assert!(b"zomg" == buf.bytes());
                        reactor.shutdown();
                    }
                    Err(e) => fail!("client sock failed to read; err={}", e),
                }
            }
            _ => fail!("received unknown token {}", tok)
        }
    }

    fn writable(&mut self, _reactor: &mut TestReactor, tok: Token) {
        match tok {
            SERVER => fail!("received writable for token 0"),
            CLIENT => debug!("client connected"),
            _ => fail!("received unknown token {}", tok)
        }
    }

    fn timeout(&mut self, _reactor: &mut TestReactor, mut sock: TcpSocket) {
        sock.write(&mut buf::wrap(b"zomg"))
            .unwrap().unwrap();
    }
}

#[test]
pub fn test_close_on_drop() {
    let mut reactor = Reactor::new().unwrap();

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();

    let srv = srv.bind(&addr).unwrap();

    info!("listening for connections");
    reactor.listen(&srv, 256u, SERVER).unwrap();

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    reactor.connect(&sock, &addr, CLIENT).unwrap();

    // Start the reactor
    reactor.run(TestHandler::new(srv, sock))
        .ok().expect("failed to execute reactor");
}
