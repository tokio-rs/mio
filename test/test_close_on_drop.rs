use mio::*;
use super::localhost;

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

impl Handler<uint> for TestHandler {
    fn readable(&mut self, reactor: &mut Reactor<uint>, tok: uint) {
        match tok {
            0 => {
                debug!("server connection ready for accept");
                let _ = self.srv.accept().unwrap().unwrap();
            }
            1 => {
                debug!("client readable");
                let mut buf = RWIobuf::new(1024);
                match self.cli.read(&mut buf) {
                    Err(e) if e.is_eof() => reactor.shutdown(),
                    _ => fail!("the client socket should not be readable")
                }
            }
            _ => fail!("received unknown token {}", tok)
        }
    }

    fn writable(&mut self, _reactor: &mut Reactor<uint>, tok: uint) {
        match tok {
            0 => fail!("received writable for token 0"),
            1 => {
                debug!("client connected");
            }
            _ => fail!("received unknown token {}", tok)
        }
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
    reactor.listen(&srv, 256u, 0u).unwrap();

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    reactor.connect(&sock, &addr, 1u).unwrap();

    // Start the reactor
    reactor.run(TestHandler::new(srv, sock))
        .ok().expect("failed to execute reactor");
}
