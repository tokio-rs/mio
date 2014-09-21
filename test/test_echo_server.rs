use mio::*;
use super::localhost;
use std::mem;
use collections::Deque;
use collections::ringbuf::RingBuf;

pub trait Context {
    fn make_iobuf(&mut self, len: uint) -> RWIobuf<'static>;
    fn save_iobuf(&mut self, buf: RWIobuf<'static>);

    fn send(&mut self, buf: RWIobuf<'static>);
}

pub trait Connection {
    /// Return true if we should stop reading from the socket. This can be used
    /// to push back on the sender.
    fn on_read(&mut self, ctx: &mut Context, buf: RWIobuf<'static>);
}

struct EchoConnection;

impl EchoConnection {
    fn new() -> EchoConnection {
        EchoConnection
    }
}

impl Connection for EchoConnection {
    fn on_read(&mut self, ctx: &mut Context, buf: RWIobuf<'static>) {
        debug!("server read: {}", buf);
        ctx.send(buf);
    }
}

static READ_BUF_SIZE:        uint = 4096;
static MAX_QUEUE_SIZE:       uint = 32;
static MAX_IOBUF_CACHE_SIZE: uint = 32;

struct SimpleServerContext {
    sock:        TcpSocket,
    readable:    bool,
    writable:    bool,
    in_buf:      RWIobuf<'static>,
    send_queue:  RingBuf<RWIobuf<'static>>,
    iobuf_cache: Vec<RWIobuf<'static>>,
}

impl SimpleServerContext {
    fn new(sock: TcpSocket) -> SimpleServerContext {
        SimpleServerContext {
            sock:        sock,
            readable:    false,
            writable:    false,
            in_buf:      RWIobuf::new(READ_BUF_SIZE),
            send_queue:  RingBuf::new(),
            iobuf_cache: Vec::new(),
        }
    }

    fn readable<C: Connection>(&mut self, connection: &mut C) -> MioResult<()> {
        self.readable = true;
        self.tick(connection)
    }

    fn writable<C: Connection>(&mut self, connection: &mut C) -> MioResult<()> {
        self.writable = true;
        self.tick(connection)
    }

    fn can_continue(&self) -> bool {
        let send_queue_len = self.send_queue.len();
        debug!("send queue len: {}", send_queue_len);
        debug!("readable: {}", self.readable);
        debug!("writable: {}", self.writable);

        // readable, and still room on the send queue.
        (self.readable && send_queue_len <= MAX_QUEUE_SIZE)
        // writable, and there's still stuff to send.
     || (self.writable && send_queue_len != 0)
    }

    fn tick<C: Connection>(&mut self, connection: &mut C) -> MioResult<()> {
        while self.can_continue() {
            try!(self.fill_buf(connection));
            try!(self.flush_buf());
        }

        Ok(())
    }

    fn fill_buf<C: Connection>(&mut self, connection: &mut C) -> MioResult<()> {
        if !self.readable {
            return Ok(());
        }

        let res = try!(self.sock.read(&mut self.in_buf));

        if res.would_block() {
            self.readable = false;
        }

        let new_iobuf = self.make_iobuf(READ_BUF_SIZE);
        let mut buf = mem::replace(&mut self.in_buf, new_iobuf);
        buf.flip_lo();

        if !buf.is_empty() {
            connection.on_read(self, buf);
        } else {
            self.save_iobuf(buf);
        }

        Ok(())
    }

    fn flush_buf(&mut self) -> MioResult<()> {
        if !self.writable {
            return Ok(());
        }

        let mut drop_head = false;

        match self.send_queue.front_mut() {
            Some(buf) => {
                debug!("trying to send: {}", buf);
                let res = try!(self.sock.write(buf));

                if res.would_block() {
                    self.writable = false;
                }

                debug!("what's left: {}", buf);

                if buf.is_empty() { drop_head = true; }
            },
            None => {}
        }

        if drop_head {
            let first_elem = self.send_queue.pop_front().unwrap();
            self.save_iobuf(first_elem);
        }

        Ok(())
    }
}

impl Context for SimpleServerContext {
    fn make_iobuf(&mut self, len: uint) -> RWIobuf<'static> {
        match self.iobuf_cache.pop() {
            Some(mut buf) => {
                if len <= buf.len() {
                    buf.resize(len).unwrap();
                    buf
                } else {
                    warn!("Saved Iobuf is too short for request, and we need to mint a new, bigger iobuf. Try to make all requested Iobufs the same size.");
                    RWIobuf::new(len)
                }
            },
            None => {
                warn!("Allocating a new Iobuf for the Iobuf cache. If you see this message a lot, except during startup, either increase the size of the iobuf cache of allocate less iobufs!");
                RWIobuf::new(len)
            },
        }
    }

    fn save_iobuf(&mut self, mut buf: RWIobuf<'static>) {
        if self.iobuf_cache.len() >= MAX_IOBUF_CACHE_SIZE {
            warn!("Iobuf cache filled up. This is likely going to cause useless allocations. Either increase the size of the iobuf cache, or allocate less iobufs!");
            drop(buf);
        } else {
            buf.reset();
            self.iobuf_cache.push(buf);
        }
    }

    fn send(&mut self, buf: RWIobuf<'static>) {
        self.send_queue.push(buf);
    }
}

struct EchoServer {
    sock: TcpAcceptor,
    conns: Slab<(SimpleServerContext, EchoConnection)>,
}

impl EchoServer {
    fn accept(&mut self, reactor: &mut Reactor<uint>) {
        debug!("server accepting socket");
        let sock = self.sock.accept().unwrap().unwrap();
        let tok = self.conns.insert((SimpleServerContext::new(sock), EchoConnection::new()))
            .ok().expect("could not add connectiont o slab");

        // Register the connection
        reactor.register(&self.conns[tok].ref0().sock, 2 + tok)
            .ok().expect("could not register socket with reactor");
    }

    fn conn_readable(&mut self, tok: uint) {
        debug!("server conn readable; tok={}", tok);
        let &(ref mut ctx, ref mut conn) = self.conn(tok);
        ctx.readable(conn).unwrap();
    }

    fn conn_writable(&mut self, tok: uint) {
        debug!("server conn writable; tok={}", tok);
        let &(ref mut ctx, ref mut conn) = self.conn(tok);
        ctx.writable(conn).unwrap();
    }

    fn conn<'a>(&'a mut self, tok: uint) -> &'a mut (SimpleServerContext, EchoConnection) {
        &mut self.conns[tok - 2]
    }
}

struct EchoClient {
    sock: TcpSocket,
    msgs: Vec<&'static str>,
    tx: ROIobuf<'static>,
    rx: ROIobuf<'static>,
    buf: RWIobuf<'static>,
    writable: bool
}

// Sends a message and expects to receive the same exact message, one at a time
impl EchoClient {
    fn new(sock: TcpSocket, mut msgs: Vec<&'static str>) -> EchoClient {
        let curr = msgs.remove(0).expect("At least one message is required");

        EchoClient {
            sock: sock,
            msgs: msgs,
            tx: ROIobuf::from_str(curr),
            rx: ROIobuf::from_str(curr),
            buf: RWIobuf::new(1024),
            writable: false
        }
    }

    fn readable(&mut self, reactor: &mut Reactor<uint>) {
        debug!("client socket readable");

        loop {
            let res = match self.sock.read(&mut self.buf) {
                Ok(r) => r,
                Err(e) => fail!("not implemented; client err={}", e)
            };

            // prepare for reading
            self.buf.flip_lo();

            while !self.buf.is_empty() {
                let actual: u8 = self.buf.consume_be().unwrap();
                let expect: u8 = self.rx.consume_be().unwrap();

                assert_eq!(actual, expect);
            }

            self.buf.reset();

            if self.rx.is_empty() {
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

    fn next_msg(&mut self, reactor: &mut Reactor<uint>) -> MioResult<()> {
        let curr = match self.msgs.remove(0) {
            Some(msg) => msg,
            None => {
                reactor.shutdown();
                return Ok(());
            }
        };

        debug!("client prepping next message");
        self.tx = ROIobuf::from_str(curr);
        self.rx = ROIobuf::from_str(curr);

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

impl Handler<uint> for EchoHandler {
    fn readable(&mut self, reactor: &mut Reactor<uint>, token: uint) {
        match token {
            0 => self.server.accept(reactor),
            1 => self.client.readable(reactor),
            i => self.server.conn_readable(i)
        }
    }

    fn writable(&mut self, _reactor: &mut Reactor<uint>, token: uint) {
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
    reactor.run(&mut EchoHandler::new(srv, sock, vec!["foo", "bar"]))
        .ok().expect("failed to execute reactor");

}
