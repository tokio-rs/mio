use mio::*;
use mio::buf::{ByteBuf, RingBuf, SliceBuf};
use super::localhost;

type TestReactor = Reactor<uint, uint, ()>;

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
        self.echo()
    }

    fn writable(&mut self) -> MioResult<()> {
        self.writable = true;
        self.echo()
    }

    fn can_continue(&self) -> bool {
        (self.readable && !self.buf.is_full()) ||
            (self.writable && !self.buf.is_empty())
    }

    fn echo(&mut self) -> MioResult<()> {
        while self.can_continue() {
            try!(self.fill_buf());
            try!(self.flush_buf());
        }

        Ok(())
    }

    fn fill_buf(&mut self) -> MioResult<()> {
        if !self.readable {
            return Ok(());
        }

        debug!("server filling buf");
        self.sock.read(&mut self.buf.writer())
            .map(|res| {
                if res.would_block() {
                    debug!("  WOULDBLOCK");
                    self.readable = false;
                }
            })
    }

    fn flush_buf(&mut self) -> MioResult<()> {
        if !self.writable {
            return Ok(());
        }

        debug!("server flushing buf");
        self.sock.write(&mut self.buf.reader())
            .map(|res| {
                if res.would_block() {
                    debug!("  WOULDBLOCK");
                    self.writable = false;
                }
            })
    }
}

struct EchoServer {
    sock: TcpAcceptor,
    conns: Slab<EchoConn>
}

impl EchoServer {
    fn accept(&mut self, reactor: &mut TestReactor) {
        debug!("server accepting socket");
        let sock = self.sock.accept().unwrap().unwrap();
        let conn = EchoConn::new(sock);
        let tok = self.conns.insert(conn)
            .ok().expect("could not add connectiont o slab");

        // Register the connection
        reactor.register(&self.conns[tok].sock, 2 + tok)
            .ok().expect("could not register socket with reactor");
    }

    fn conn_readable(&mut self, tok: uint) {
        debug!("server conn readable; tok={}", tok);
        self.conn(tok).readable().unwrap();
    }

    fn conn_writable(&mut self, tok: uint) {
        debug!("server conn writable; tok={}", tok);
        self.conn(tok).writable().unwrap();
    }

    fn conn<'a>(&'a mut self, tok: uint) -> &'a mut EchoConn {
        &mut self.conns[tok - 2]
    }
}

struct EchoClient {
    sock: TcpSocket,
    msgs: Vec<&'static str>,
    tx: SliceBuf<'static>,
    rx: SliceBuf<'static>,
    buf: ByteBuf,
    writable: bool
}


// Sends a message and expects to receive the same exact message, one at a time
impl EchoClient {
    fn new(sock: TcpSocket, mut msgs: Vec<&'static str>) -> EchoClient {
        let curr = msgs.remove(0).expect("At least one message is required");

        EchoClient {
            sock: sock,
            msgs: msgs,
            tx: SliceBuf::wrap(curr.as_bytes()),
            rx: SliceBuf::wrap(curr.as_bytes()),
            buf: ByteBuf::new(1024),
            writable: false
        }
    }

    fn readable(&mut self, reactor: &mut TestReactor) {
        debug!("client socket readable");

        loop {
            let res = match self.sock.read(&mut self.buf) {
                Ok(r) => r,
                Err(e) => fail!("not implemented; client err={}", e)
            };

            // prepare for reading
            self.buf.flip();

            while self.buf.has_remaining() {
                let actual = self.buf.read_byte().unwrap();
                let expect = self.rx.read_byte().unwrap();

                assert!(actual == expect);
            }

            self.buf.clear();

            if !self.rx.has_remaining() {
                self.next_msg(reactor).unwrap();
            }

            // Nothing else to do this round
            if res.would_block() {
                return;
            }
        }
    }

    fn writable(&mut self) {
        debug!("client socket writable");

        self.writable = true;
        self.flush_msg().unwrap();
    }

    fn flush_msg(&mut self) -> MioResult<()> {
        if !self.writable {
            return Ok(());
        }

        self.sock.write(&mut self.tx)
            .map(|res| {
                if res.would_block() {
                    debug!("client flushing buf; WOULDBLOCK");
                    self.writable = false
                } else {
                    debug!("client flushed buf");
                }
            })
    }

    fn next_msg(&mut self, reactor: &mut TestReactor) -> MioResult<()> {
        let curr = match self.msgs.remove(0) {
            Some(msg) => msg,
            None => {
                reactor.shutdown();
                return Ok(());
            }
        };

        debug!("client prepping next message");
        self.tx = SliceBuf::wrap(curr.as_bytes());
        self.rx = SliceBuf::wrap(curr.as_bytes());

        self.flush_msg()
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

impl Handler<uint, uint, ()> for EchoHandler {
    fn readable(&mut self, reactor: &mut TestReactor, token: uint) {
        match token {
            0 => self.server.accept(reactor),
            1 => self.client.readable(reactor),
            i => self.server.conn_readable(i)
        }
    }

    fn writable(&mut self, _reactor: &mut TestReactor, token: uint) {
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

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();

    let srv = srv.bind(&addr).unwrap();

    info!("listen for connections");
    reactor.listen(&srv, 256u, 0u).unwrap();

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    reactor.connect(&sock, &addr, 1u).unwrap();

    // Start the reactor
    reactor.run(EchoHandler::new(srv, sock, vec!["foo", "bar"]))
        .ok().expect("failed to execute reactor");

}
