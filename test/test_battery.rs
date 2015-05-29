use {sleep_ms};
use mio::*;
use mio::tcp::*;
use mio::util::Slab;
use super::localhost;
use std::collections::LinkedList;
use std::{io, thread};

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

struct EchoConn {
    sock: TcpStream,
    token: Option<Token>,
    count: u32,
    buf: Vec<u8>
}

impl EchoConn {
    fn new(sock: TcpStream) -> EchoConn {
        let mut ec =
        EchoConn {
            sock: sock,
            token: None,
            buf: Vec::with_capacity(22),
            count: 0
        };
        unsafe { ec.buf.set_len(22) };
        ec
    }

    fn writable(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        event_loop.reregister(&self.sock, self.token.unwrap(), Interest::readable(), PollOpt::edge() | PollOpt::oneshot())
    }

    fn readable(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        loop {
            match self.sock.try_read(&mut self.buf[..]) {
                Ok(None) => {
                    break;
                }
                Ok(Some(_)) => {
                    self.count += 1;
                    if self.count % 10000 == 0 {
                        debug!("Received {} messages", self.count);
                    }
                    if self.count == 1_000_000 {
                        event_loop.shutdown();
                    }
                }
                Err(_) => {
                    break;
                }

            };
        }

        event_loop.reregister(&self.sock, self.token.unwrap(), Interest::readable(), PollOpt::edge() | PollOpt::oneshot())
    }
}

struct EchoServer {
    sock: TcpListener,
    conns: Slab<EchoConn>
}

impl EchoServer {
    fn accept(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        debug!("server accepting socket");

        let sock = self.sock.accept().unwrap().unwrap();
        let conn = EchoConn::new(sock,);
        let tok = self.conns.insert(conn)
            .ok().expect("could not add connection to slab");

        // Register the connection
        self.conns[tok].token = Some(tok);
        event_loop.register_opt(&self.conns[tok].sock, tok, Interest::readable(), PollOpt::edge() | PollOpt::oneshot())
            .ok().expect("could not register socket with event loop");

        Ok(())
    }

    fn conn_readable(&mut self, event_loop: &mut EventLoop<Echo>, tok: Token) -> io::Result<()> {
        debug!("server conn readable; tok={:?}", tok);
        self.conn(tok).readable(event_loop)
    }

    fn conn_writable(&mut self, event_loop: &mut EventLoop<Echo>, tok: Token) -> io::Result<()> {
        debug!("server conn writable; tok={:?}", tok);
        self.conn(tok).writable(event_loop)
    }

    fn conn<'a>(&'a mut self, tok: Token) -> &'a mut EchoConn {
        &mut self.conns[tok]
    }
}

struct EchoClient {
    sock: TcpStream,
    backlog: LinkedList<String>,
    token: Token,
    count: u32
}


// Sends a message and expects to receive the same exact message, one at a time
impl EchoClient {
    fn new(sock: TcpStream, tok: Token) -> EchoClient {

        EchoClient {
            sock: sock,
            backlog: LinkedList::new(),
            token: tok,
            count: 0
        }
    }

    fn readable(&mut self, _event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        Ok(())
    }

    fn writable(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        debug!("client socket writable");

        while self.backlog.len() > 0 {
            match self.sock.try_write(self.backlog.front().unwrap().as_bytes()) {
                Ok(None) => {
                    break;
                }
                Ok(Some(_)) => {
                    self.backlog.pop_front();
                    self.count += 1;
                    if self.count % 10000 == 0 {
                        debug!("Sent {} messages", self.count);
                    }
                }
                Err(e) => { debug!("not implemented; client err={:?}", e); break; }
            }
        }
        if self.backlog.len() > 0 {
            event_loop.reregister(&self.sock, self.token, Interest::writable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
        }

        Ok(())
    }
}

struct Echo {
    server: EchoServer,
    client: EchoClient,
}

impl Echo {
    fn new(srv: TcpListener, client: TcpStream) -> Echo {
        Echo {
            server: EchoServer {
                sock: srv,
                conns: Slab::new_starting_at(Token(2), 128),
            },
            client: EchoClient::new(client, CLIENT),
        }
    }
}

impl Handler for Echo {
    type Timeout = usize;
    type Message = String;

    fn readable(&mut self, event_loop: &mut EventLoop<Echo>, token: Token, hint: ReadHint) {
        assert_eq!(hint, ReadHint::data());

        match token {
            SERVER => self.server.accept(event_loop).unwrap(),
            CLIENT => self.client.readable(event_loop).unwrap(),
            i => self.server.conn_readable(event_loop, i).unwrap()
        };
    }

    fn writable(&mut self, event_loop: &mut EventLoop<Echo>, token: Token) {
        match token {
            SERVER => panic!("received writable for token 0"),
            CLIENT => self.client.writable(event_loop).unwrap(),
            _ => self.server.conn_writable(event_loop, token).unwrap()
        };
    }

    fn notify(&mut self, event_loop: &mut EventLoop<Echo>, msg: String) {
        match self.client.sock.try_write(msg.as_bytes()) {
            Ok(Some(n)) => {
                self.client.count += 1;
                if self.client.count % 10000 == 0 {
                    debug!("Sent {} bytes:   count {}", n, self.client.count);
                }
            },

            _ => {
                self.client.backlog.push_back(msg);
                event_loop.reregister(
                    &self.client.sock,
                    self.client.token,
                    Interest::writable(),
                    PollOpt::edge() | PollOpt::oneshot()).unwrap();
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

    let addr = localhost();

    let srv = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    srv.set_reuseaddr(true).unwrap();
    srv.bind(&addr).unwrap();

    let srv = srv.listen(256).unwrap();

    info!("listen for connections");
    event_loop.register_opt(&srv, SERVER, Interest::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    let (sock, _) = TcpSocket::v4().unwrap()
        .connect(&addr).unwrap();

    // Connect to the server
    event_loop.register_opt(&sock, CLIENT, Interest::writable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
    let chan = event_loop.channel();

    let go = move || {
        let mut i = 1_000_000;

        sleep_ms(1_000);

        let message = "THIS IS A TEST MESSAGE".to_string();
        while i > 0 {
            chan.send(message.clone()).unwrap();
            i -= 1;
            if i % 10000 == 0 {
                debug!("Enqueued {} messages", 1_000_000 - i);
            }
        }
    };

    let t = thread::spawn(go);

    // Start the event loop
    event_loop.run(&mut Echo::new(srv, sock)).unwrap();
    t.join().unwrap();
}
