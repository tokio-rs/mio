use mio::*;
use mio::buf::{RingBuf, SliceBuf};

#[deriving(Show)]
struct EchoConn {
    sock: TcpSocket,
    readable: bool,
    writable: bool,
    buf: RingBuf
}

impl EchoConn {
    fn new(sock: TcpSocket) -> EchoConn {
        EchoConn {
            sock: sock,
            readable: false,
            writable: false,
            buf: RingBuf::new(1024)
        }
    }

    fn readable(&mut self) -> MioResult<()> {
        self.readable = true;

        loop {
            try!(self.fill_buf());

            if self.writable {
                try!(self.flush_buf());
            }

            if !self.readable || self.buf.is_full() {
                return Ok(());
            }
        }
    }

    fn writable(&mut self) {
        self.writable = true;

        if self.readable {
            self.flush_buf();
        }
    }

    fn fill_buf(&mut self) -> MioResult<()> {
        let mut dst = self.buf.writer();

        while dst.has_remaining() {
            match self.sock.read(&mut dst) {
                Ok(_) => {}
                Err(e) if e.is_eof() => {
                    // TODO: Handle eof
                }
                Err(e) if e.is_would_block() => {
                    self.readable = false;
                    return Ok(());
                }
                e => return e
            }
        }

        Ok(())
    }

    fn flush_buf(&mut self) -> MioResult<()> {
        unimplemented!()
        // let writer = self.
    }
}

struct EchoServer {
    sock: TcpAcceptor,
    conns: Slab<EchoConn>
}

impl EchoServer {
    fn accept(&mut self, reactor: &mut Reactor<uint>) {
        let conn = EchoConn::new(self.sock.accept().unwrap());

        // Register the connection
        reactor.register(conn.sock, 2 + self.conns.insert(conn).unwrap());
    }

    fn conn_readable(&mut self, tok: uint) {
    }

    fn conn_writable(&mut self, tok: uint) {
    }
}

struct EchoClient {
    sock: TcpSocket,
    msgs: Vec<&'static str>,
    tx: SliceBuf<'static>,
    rx: SliceBuf<'static>
}


// Sends a message and expects to receive the same exact message, one at a time
impl EchoClient {
    fn new(sock: TcpSocket, mut msgs: Vec<&'static str>) -> EchoClient {
        let curr = msgs.remove(0).unwrap();

        EchoClient {
            sock: sock,
            msgs: msgs,
            tx: SliceBuf::wrap(curr.as_bytes()),
            rx: SliceBuf::wrap(curr.as_bytes())
        }
    }

    fn readable(&mut self) {
        debug!("client socket readable");
        unimplemented!()
    }

    fn writable(&mut self) {
        debug!("client socket writable");
        unimplemented!()
    }
}

struct EchoHandler {
    server: EchoServer,
    client: EchoClient,
}

impl EchoHandler {
    fn new(srv: TcpAcceptor, client: TcpSocket, msgs: Vec<&'static str>) -> EchoHandler {
        EchoHandler {
            server: EchoServer {
                sock: srv,
                conns: Slab::new(128)
            },

            client: EchoClient::new(client, msgs)
        }
    }
}

impl Handler<uint> for EchoHandler {
    fn readable(&mut self, reactor: &mut Reactor<uint>, token: uint) {
        debug!("handler readable; token={}", token);

        match token {
            0 => self.server.accept(reactor),
            1 => self.client.readable(),
            i => self.server.conn_readable(i)
        }
    }

    fn writable(&mut self, reactor: &mut Reactor<uint>, token: uint) {
        debug!("handler writable; token={}", token);

        match token {
            0 => fail!("received writable for token 0"),
            1 => self.client.writable(),
            i => self.server.conn_writable(i)
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
    reactor.run(EchoHandler::new(srv, sock, vec!["foo", "bar"]));
}
