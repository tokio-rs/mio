use mio::*;
use super::localhost;

struct ServerHandler {
    srv: TcpAcceptor,
}

impl ServerHandler {
    fn new(srv: TcpAcceptor) -> ServerHandler {
        ServerHandler {
            srv: srv,
        }
    }
}

impl Handler for ServerHandler {
    fn readable(&mut self, _reactor: &mut Reactor) -> MioResult<()> {
        debug!("server connection ready for accept");
        let _ = self.srv.accept().unwrap().unwrap();
        Ok(())
    }

    fn writable(&mut self, _reactor: &mut Reactor) -> MioResult<()> {
        fail!("received writable for server's accept socket");
    }
}

struct ClientHandler {
    cli: TcpSocket,
}

impl ClientHandler {
    fn new(cli: TcpSocket) -> ClientHandler {
        ClientHandler {
            cli: cli,
        }
    }
}

impl Handler for ClientHandler {
    fn readable(&mut self, reactor: &mut Reactor) -> MioResult<()> {
        debug!("client readable");
        let mut buf = RWIobuf::new(1024);
        match self.cli.read(&mut buf) {
            Err(e) if e.is_eof() => reactor.shutdown(),
            _ => fail!("the client socket should not be readable"),
        }

        Ok(())
    }

    fn writable(&mut self, _reactor: &mut Reactor) -> MioResult<()> {
        debug!("client connected");
        Ok(())
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

    {
        let srv = srv.bind(&addr).unwrap();

        info!("listening for connections");
        reactor.listen(srv, 256u, |srv| ServerHandler::new(srv)).unwrap();
    }

    {
        let sock = TcpSocket::v4().unwrap();

        // Connect to the server
        reactor.connect(sock, &addr, |sock| ClientHandler::new(sock)).unwrap();
    }

    // Start the reactor
    reactor.run().ok().expect("failed to execute reactor");
}
