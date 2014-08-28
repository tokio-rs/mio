/*
use mio::*;

struct EchoConn {
    sock: TcpSocket,
    writable: bool,
    buf: Vec<u8>
}

impl EchoConn {
    fn readable(&mut self) {

    }

    fn writable(&mut self) {
        self.writable = true;

        // If any data got buffered up, flush it
        while self.buf.len() > 0 {
            match self.sock.write(self.buf.as_slice()) {
                Ok(nwrite) => {
                }
                Err(e) if e.is_would_block() {
                    self.writable = false;
                    return;
                }
                Err(e) => fail!("failed to write to sock; err={}", e)
            }
        }

        // Flush the buffer
    }
}

struct EchoServer {
    sock: TcpAcceptor,
    conns: Vec<EchoConn>
}

struct EchoClient {
    sock: TcpSocket,
    msgs: Vec<String>,
    curr: Option<String>
}

struct EchoHandler {
    server: EchoServer,
    client: EchoClient,
}

impl EchoHandler {
    fn new(srv: TcpAcceptor, client: TcpSocket, msgs: Vec<String>) -> EchoHandler {
        EchoHandler {
            server: EchoServer {
                sock: srv,
                conns: vec![]
            },
            client: EchoClient {
                sock: client,
                msgs: msgs,
                curr: None
            }
        }
    }
}

impl Handler<uint> for EchoHandler {
    fn readable(&mut self, reactor: &mut Reactor, token: uint) {
        info!("readable; token={}", token);

        match token {
            0 => {
                let sock = self.srv.accept().unwrap();

                self.socks.push(sock);
                reactor.register(sock, 1 + self.socks.len());
            }
            1 => {
                println!("client readable");
            }
            i => {
                println!("srv socket");
            }
        }
    }

    fn writable(&mut self, reactor: &mut Reactor, token: uint) {
        info!("writable; token={}", token);

        match token {
            1 => {
                self.client.write(b"HELLO").unwrap();
            }
            2 => {},
            _ => fail!("unexpected writable event; token={}", token)
        }
    }
}

#[test]
pub fn test_echo_server() {
    let mut reactor = Reactor::new().unwrap();

    let addr = SockAddr::parse("127.0.0.1:8080")
        .expect("could not parse InetAddr");

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();

    let srv = srv.bind(&addr).unwrap();

    info!("listen for connections");
    reactor.listen(srv, 256u, 0u);

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    reactor.connect(sock, &addr, 1u).unwrap();

    // Start the reactor
    reactor.run(EchoHandler::new(srv, sock));
}
*/
