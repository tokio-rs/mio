use mio::*;
use mio::net::*;
use mio::net::tcp::*;
use mio::buf::{ByteBuf, RingBuf, SliceBuf};
use mio::util::Slab;
use super::localhost;
use mio::event_ctx as evt; 
use mio::event_ctx::IoEventKind;
//use mio::io;
type TestEventLoop = EventLoop<uint, ()>;

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

struct EchoConn {
        sock: TcpSocket,
        buf: ByteBuf,
        interest: IoEventCtx 
}

impl EchoConn {
  fn new(sock: TcpSocket) -> EchoConn {
    EchoConn {
        sock: sock,
        buf: ByteBuf::new(2048),
        interest: IoEventCtx::new(evt::IOEDGE, Token(-1))
    }
  }

  fn writable(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {
    debug!("CON : writing buf = {}", self.buf.bytes());
    match self.sock.write(&mut self.buf) {
      Ok(io::WouldBlock) => { debug!("client flushing buf; WOULDBLOCK");
                              self.interest.set_writable(true); }
            Ok(Ready(r)) => { debug!("CONN : we wrote {} bytes!", r);
                              self.buf.clear();
                              self.interest.set_readable(true);
                              self.interest.set_writable(false); }
                  Err(e) => debug!("not implemented; client err={}", e)
     } 
    event_loop.reregister(&self.sock, &self.interest)
  }

  fn readable(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {

    match self.sock.read(&mut self.buf) {
      Ok(io::WouldBlock) => panic!("We just got readable, but were unable to read from the socket?"),
            Ok(Ready(r)) => { debug!("CONN : we read {} bytes!", r);
                              self.interest.set_readable(false);
                              self.interest.set_writable(true); }
                  Err(e) => { debug!("not implemented; client err={}", e)
                              self.interest.set_readable(false); }
                              
    };
    

    // prepare to provide this to writable 
    self.buf.flip();
    
    debug!("CON : read buf = {}", self.buf.bytes());

    event_loop.reregister(&self.sock, &self.interest)
  }
}

struct EchoServer {
sock: TcpAcceptor,
        conns: Slab<EchoConn>
}

impl EchoServer {
  fn accept(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {
    debug!("server accepting socket");
    let sock = self.sock.accept().unwrap().unwrap();
    let conn = EchoConn::new(sock,);
    let tok = self.conns.insert(conn)
      .ok().expect("could not add connectiont o slab");

    // Register the connection
    let evt = IoEventCtx::new(evt::IOREADABLE | evt::IOEDGE, tok);
    self.conns[tok].interest = evt;
    event_loop.register(&self.conns[tok].sock, &evt)
      .ok().expect("could not register socket with event loop");

    
    Ok(())
  }

  fn conn_readable(&mut self, event_loop: &mut TestEventLoop, tok: Token) -> MioResult<()> {
    debug!("server conn readable; tok={}", tok);
    self.conn(tok).readable(event_loop)
  }

  fn conn_writable(&mut self, event_loop: &mut TestEventLoop, tok: Token) -> MioResult<()> {
    debug!("server conn writable; tok={}", tok);
    self.conn(tok).writable(event_loop)
  }

  fn conn<'a>(&'a mut self, tok: Token) -> &'a mut EchoConn {
    &mut self.conns[tok]
  }
}

struct EchoClient {
sock: TcpSocket,
        msgs: Vec<&'static str>,
        tx: SliceBuf<'static>,
        rx: SliceBuf<'static>,
        buf: ByteBuf,
        interest: IoEventCtx
}


// Sends a message and expects to receive the same exact message, one at a time
impl EchoClient {
  fn new(sock: TcpSocket, tok: Token,  mut msgs: Vec<&'static str>) -> EchoClient {
    let curr = msgs.remove(0).expect("At least one message is required");

    EchoClient {
        sock: sock,
        msgs: msgs,
        tx: SliceBuf::wrap(curr.as_bytes()),
        rx: SliceBuf::wrap(curr.as_bytes()),
        buf: ByteBuf::new(2048),
        interest: IoEventCtx::new(evt::IOEDGE, tok)
    }
  }

  fn readable(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {
    debug!("client socket readable");

    let res = match self.sock.read(&mut self.buf) {
      Ok(io::WouldBlock) => panic!("We just got readable, but were unable to read from the socket?"),
        Ok(Ready(r)) => { debug!("CLIENT : We read {} bytes!", r); }
      Err(e) => panic!("not implemented; client err={}", e)
    };

    // prepare for reading
    self.buf.flip();

    debug!("CLIENT : buf = {} -- rx = {}", self.buf.bytes(), self.rx.bytes());
    while self.buf.has_remaining() {
      let actual = self.buf.read_byte().unwrap();
      let expect = self.rx.read_byte().unwrap();

      assert!(actual == expect);
    }

    self.buf.clear();

    self.interest.set_readable(false);
    if !self.rx.has_remaining() {
      self.next_msg(event_loop).unwrap();
    }
    event_loop.reregister(&self.sock, &self.interest)
  }

  fn writable(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {
    debug!("client socket writable");
    
    match self.sock.write(&mut self.tx) {
      Ok(io::WouldBlock) => { debug!("client flushing buf; WOULDBLOCK");
                              self.interest.set_writable(true); }
            Ok(Ready(r)) => { debug!("CLIENT : we wrote {} bytes!", r);
                              self.interest.set_readable(true);
                              self.interest.set_writable(false); }
                  Err(e) => debug!("not implemented; client err={}", e)
     } 

    event_loop.reregister(&self.sock, &self.interest)
  }

  fn next_msg(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {
    let curr = match self.msgs.remove(0) {
      Some(msg) => msg,
        None => {
          event_loop.shutdown();
          return Ok(());
        }
    };

    debug!("client prepping next message");
    self.tx = SliceBuf::wrap(curr.as_bytes());
    self.rx = SliceBuf::wrap(curr.as_bytes());

    self.interest.set_writable(true);
    event_loop.reregister(&self.sock, &self.interest)
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
        conns: Slab::new_starting_at(Token(2), 128)
      },
      client: EchoClient::new(client, CLIENT, msgs)
    }
  }
}

impl Handler<uint, ()> for EchoHandler {
  fn readable(&mut self, event_loop: &mut TestEventLoop, token: Token, hint: ReadHint) {
    assert_eq!(hint, DATAHINT);

    match token {
      SERVER => self.server.accept(event_loop).unwrap(),
      CLIENT => self.client.readable(event_loop).unwrap(),
           i => self.server.conn_readable(event_loop, i).unwrap()
    };
  }

  fn writable(&mut self, event_loop: &mut TestEventLoop, token: Token) {
    match token {
      SERVER => panic!("received writable for token 0"),
      CLIENT => self.client.writable(event_loop).unwrap(),
           i => self.server.conn_writable(event_loop, i).unwrap()
    };
  }
}

#[test]
pub fn test_echo_server() {
  debug!("Starting TEST_ECHO_SERVER");
  let mut event_loop = EventLoop::new().unwrap();

  let addr = SockAddr::parse(localhost().as_slice())
    .expect("could not parse InetAddr");

  let srv = TcpSocket::v4().unwrap();

  info!("setting re-use addr");
  srv.set_reuseaddr(true).unwrap();

  let srv = srv.bind(&addr).unwrap()
    .listen(256u).unwrap();

  info!("listen for connections");
  let evt = IoEventCtx::new(evt::IOREADABLE | evt::IOEDGE, SERVER);
  event_loop.register(&srv, &evt).unwrap();

  let sock = TcpSocket::v4().unwrap();

  // Connect to the server
  let cevt = IoEventCtx::new(evt::IOWRITABLE | evt::IOEDGE, CLIENT);
  event_loop.register(&sock, &cevt).unwrap();
  sock.connect(&addr).unwrap();

  // Start the event loop
  event_loop.run(EchoHandler::new(srv, sock, vec!["foo", "bar"]))
    .ok().expect("failed to execute event loop");

}
