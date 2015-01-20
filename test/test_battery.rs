use mio::*;
use mio::net::*;
use mio::net::tcp::*;
use mio::buf::{ByteBuf, SliceBuf};
use mio::util::Slab;
use super::localhost;
use mio::event as evt;
use collections::DList;
use std::thread::Thread;

type TestEventLoop = EventLoop<usize, String>;

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

struct EchoConn {
    sock: TcpSocket,
    token: Token,
    count: u32,
    buf: ByteBuf
}

impl EchoConn {
    fn new(sock: TcpSocket) -> EchoConn {
        EchoConn {
            sock: sock,
            token: Token(-1),
            buf: ByteBuf::new(10000),
            count: 0
        }
    }

    fn writable(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {

        event_loop.reregister(&self.sock, self.token, evt::READABLE, evt::PollOpt::edge())
    }

    fn readable(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {
        match self.sock.read(&mut self.buf) {
            Ok(NonBlock::WouldBlock) => {
                panic!("We just got readable, but were unable to read from the socket?");
            }
            Ok(NonBlock::Ready(r)) => {
                debug!("CONN : we read {} bytes!", r);
                self.buf.clear();
                self.count += 1;
                if self.count % 10000 == 0 {
                    println!("Received {} messages", self.count);
                }
            }
            Err(e) => {
                debug!("not implemented; client err={:?}", e);
            }

        };

        event_loop.reregister(&self.sock, self.token, evt::READABLE, evt::PollOpt::edge())
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
            .ok().expect("could not add connection to slab");

        // Register the connection
        self.conns[tok].token = tok;
        event_loop.register_opt(&self.conns[tok].sock, tok, evt::READABLE, evt::PollOpt::edge())
            .ok().expect("could not register socket with event loop");

        Ok(())
    }

    fn conn_readable(&mut self, event_loop: &mut TestEventLoop, tok: Token) -> MioResult<()> {
        debug!("server conn readable; tok={:?}", tok);
        self.conn(tok).readable(event_loop)
    }

    fn conn_writable(&mut self, event_loop: &mut TestEventLoop, tok: Token) -> MioResult<()> {
        debug!("server conn writable; tok={:?}", tok);
        self.conn(tok).writable(event_loop)
    }

    fn conn<'a>(&'a mut self, tok: Token) -> &'a mut EchoConn {
        &mut self.conns[tok]
    }
}

struct EchoClient {
    sock: TcpSocket,
    backlog: DList<String>,
    token: Token,
    count: u32
}


// Sends a message and expects to receive the same exact message, one at a time
impl EchoClient {
    fn new(sock: TcpSocket, tok: Token) -> EchoClient {

        EchoClient {
            sock: sock,
            backlog: DList::new(),
            token: tok,
            count: 0
        }
    }

    fn readable(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {
        Ok(())
    }

    fn writable(&mut self, event_loop: &mut TestEventLoop) -> MioResult<()> {
        debug!("client socket writable");

        while self.backlog.len() > 0 {
            match self.sock.write_slice(self.backlog.front().unwrap().as_bytes()) {
                Ok(NonBlock::WouldBlock) => {
                    break;
                }
                Ok(NonBlock::Ready(r)) => {
                    self.backlog.pop_front();
                    self.count += 1;
                }
                Err(e) => { debug!("not implemented; client err={:?}", e); break; }
            }
        }
        if self.backlog.len() > 0 {
            event_loop.reregister(&self.sock, self.token, evt::WRITABLE, evt::PollOpt::edge());
        }

        Ok(())
    }
}

struct EchoHandler {
    server: EchoServer,
    client: EchoClient,
    count: u32
}

impl EchoHandler {
    fn new(srv: TcpAcceptor, client: TcpSocket) -> EchoHandler {
        EchoHandler {
            server: EchoServer {
                sock: srv,
                conns: Slab::new_starting_at(Token(2), 128),
            },
            client: EchoClient::new(client, CLIENT),
            count: 0
        }
    }
}

impl Handler<usize, String> for EchoHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, token: Token, hint: evt::ReadHint) {
        assert_eq!(hint, evt::DATAHINT);

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
            _ => self.server.conn_writable(event_loop, token).unwrap()
        };
    }

    fn notify(&mut self, event_loop: &mut TestEventLoop, msg: String) {
        match self.client.sock.write_slice(msg.as_bytes()) {
            Ok(n) => self.count += 1,
            Err(_) => {
                self.client.backlog.push_back(msg);
                event_loop.reregister(&self.client.sock, self.client.token, evt::WRITABLE, evt::PollOpt::edge());
            }
        }
    }
}

#[test]
pub fn test_echo_server() {
    debug!("Starting TEST_ECHO_SERVER");
    let config =
        EventLoopConfig {
            io_poll_timeout_ms: 1_000,
            notify_capacity: 1_048_576,
            messages_per_tick: 64,
            timer_tick_ms: 100,
            timer_wheel_size: 1_024,
            timer_capacity: 65_536,
        };
    let mut event_loop = EventLoop::configured(config).unwrap();

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();

    let srv = srv.bind(&addr).unwrap()
        .listen(256).unwrap();

    info!("listen for connections");
    event_loop.register_opt(&srv, SERVER, evt::READABLE, evt::PollOpt::edge()).unwrap();

    let sock = TcpSocket::v4().unwrap();

    // Connect to the server
    event_loop.register_opt(&sock, CLIENT, evt::WRITABLE, evt::PollOpt::edge()).unwrap();
    sock.connect(&addr).unwrap();
    let chan = event_loop.channel();

    let go = move|:| {
        let mut i = 1_000_000;

        let message = String::from_str("THIS IS A TEST MESSAGE");
        while i > 0 {
            chan.send(message.clone());
            i -= 1;
            if i % 10000 == 0 {
                println!("Sent {} messages", 1_000_000 - i);
            }
        }
    };

    let guard = Thread::spawn(go);


    // Start the event loop
    event_loop.run(EchoHandler::new(srv, sock))
        .ok().expect("failed to execute event loop");

}
