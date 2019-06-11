use crate::{localhost, TryRead, TryWrite};
use bytes::{Buf, Bytes, BytesMut};
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interests, Poll, PollOpt, Registry, Token};
use slab::Slab;
use std::io::{self, Cursor};

const SERVER: Token = Token(10_000_000);
const CLIENT: Token = Token(10_000_001);

struct EchoConn {
    sock: TcpStream,
    buf: Option<Cursor<Bytes>>,
    mut_buf: Option<BytesMut>,
    token: Option<Token>,
    interests: Option<Interests>,
}

impl EchoConn {
    fn new(sock: TcpStream) -> EchoConn {
        EchoConn {
            sock,
            buf: None,
            mut_buf: Some(BytesMut::with_capacity(2048)),
            token: None,
            interests: None,
        }
    }

    fn writable(&mut self, registry: &Registry) -> io::Result<()> {
        let mut buf = self.buf.take().unwrap();

        match self.sock.try_write_buf(&mut buf) {
            Ok(None) => {
                debug!("client flushing buf; WOULDBLOCK");

                self.buf = Some(buf);
                self.interests = match self.interests {
                    None => Some(Interests::WRITABLE),
                    Some(i) => Some(i | Interests::WRITABLE),
                };
            }
            Ok(Some(r)) => {
                debug!("CONN : we wrote {} bytes!", r);

                self.mut_buf = Some(buf.into_inner().try_mut().unwrap());

                self.interests = match self.interests {
                    None => Some(Interests::READABLE),
                    Some(i) => Some((i | Interests::READABLE) - Interests::WRITABLE),
                };
            }
            Err(e) => debug!("not implemented; client err={:?}", e),
        }

        assert!(
            self.interests.unwrap().is_readable() || self.interests.unwrap().is_writable(),
            "actual={:?}",
            self.interests
        );
        registry.reregister(
            &self.sock,
            self.token.unwrap(),
            self.interests.unwrap(),
            PollOpt::edge() | PollOpt::oneshot(),
        )
    }

    fn readable(&mut self, registry: &Registry) -> io::Result<()> {
        let mut buf = self.mut_buf.take().unwrap();
        buf.clear();

        match self.sock.try_read_buf(&mut buf) {
            Ok(None) => {
                debug!("CONN : spurious read wakeup");
                self.mut_buf = Some(buf);
            }
            Ok(Some(r)) => {
                debug!("CONN : we read {} bytes!", r);

                // prepare to provide this to writable
                self.buf = Some(Cursor::new(buf.freeze()));

                self.interests = match self.interests {
                    None => Some(Interests::WRITABLE),
                    Some(i) => Some((i | Interests::WRITABLE) - Interests::READABLE),
                }
            }
            Err(e) => {
                debug!("not implemented; client err={:?}", e);
                if self.interests == Some(Interests::READABLE) {
                    self.interests = None;
                } else if let Some(x) = self.interests.as_mut() {
                    *x -= Interests::READABLE;
                }
            }
        };

        assert!(
            self.interests.unwrap().is_readable() || self.interests.unwrap().is_writable(),
            "actual={:?}",
            self.interests
        );
        registry.reregister(
            &self.sock,
            self.token.unwrap(),
            self.interests.unwrap(),
            PollOpt::edge(),
        )
    }
}

struct EchoServer {
    sock: TcpListener,
    conns: Slab<EchoConn>,
}

impl EchoServer {
    fn accept(&mut self, registry: &Registry) -> io::Result<()> {
        debug!("server accepting socket");

        let sock = self.sock.accept().unwrap().0;
        let conn = EchoConn::new(sock);
        let tok = self.conns.insert(conn);

        // Register the connection
        self.conns[tok].token = Some(Token(tok));
        registry
            .register(
                &self.conns[tok].sock,
                Token(tok),
                Interests::READABLE,
                PollOpt::edge() | PollOpt::oneshot(),
            )
            .expect("could not register socket with event loop");

        Ok(())
    }

    fn conn_readable(&mut self, registry: &Registry, tok: Token) -> io::Result<()> {
        debug!("server conn readable; tok={:?}", tok);
        self.conn(tok).readable(registry)
    }

    fn conn_writable(&mut self, registry: &Registry, tok: Token) -> io::Result<()> {
        debug!("server conn writable; tok={:?}", tok);
        self.conn(tok).writable(registry)
    }

    fn conn(&mut self, tok: Token) -> &mut EchoConn {
        &mut self.conns[tok.into()]
    }
}

struct EchoClient {
    sock: TcpStream,
    msgs: Vec<&'static str>,
    tx: Cursor<Bytes>,
    rx: Cursor<Bytes>,
    mut_buf: Option<BytesMut>,
    token: Token,
    interests: Option<Interests>,
    shutdown: bool,
}

// Sends a message and expects to receive the same exact message, one at a time
impl EchoClient {
    fn new(sock: TcpStream, token: Token, mut msgs: Vec<&'static str>) -> EchoClient {
        let curr = msgs.remove(0);

        EchoClient {
            sock,
            msgs,
            tx: Cursor::new(Bytes::from_static(curr.as_bytes())),
            rx: Cursor::new(Bytes::from_static(curr.as_bytes())),
            mut_buf: Some(BytesMut::with_capacity(2048)),
            token,
            interests: None,
            shutdown: false,
        }
    }

    fn readable(&mut self, registry: &Registry) -> io::Result<()> {
        debug!("client socket readable");

        let mut buf = self.mut_buf.take().unwrap();
        buf.clear();

        match self.sock.try_read_buf(&mut buf) {
            Ok(None) => {
                debug!("CLIENT : spurious read wakeup");
                self.mut_buf = Some(buf);
            }
            Ok(Some(r)) => {
                debug!("CLIENT : We read {} bytes!", r);

                // prepare for reading
                let mut buf = Cursor::new(buf.freeze());

                while buf.has_remaining() {
                    let actual = buf.get_u8();
                    let expect = self.rx.get_u8();

                    assert!(actual == expect, "actual={}; expect={}", actual, expect);
                }

                self.mut_buf = Some(buf.into_inner().try_mut().unwrap());

                if self.interests == Some(Interests::READABLE) {
                    self.interests = None;
                } else if let Some(x) = self.interests.as_mut() {
                    *x -= Interests::READABLE;
                }

                if !self.rx.has_remaining() {
                    self.next_msg(registry).unwrap();
                }
            }
            Err(e) => {
                panic!("not implemented; client err={:?}", e);
            }
        };

        if let Some(x) = self.interests {
            registry.reregister(
                &self.sock,
                self.token,
                x,
                PollOpt::edge() | PollOpt::oneshot(),
            )?;
        }

        Ok(())
    }

    fn writable(&mut self, registry: &Registry) -> io::Result<()> {
        debug!("client socket writable");

        match self.sock.try_write_buf(&mut self.tx) {
            Ok(None) => {
                debug!("client flushing buf; WOULDBLOCK");
                self.interests = match self.interests {
                    None => Some(Interests::WRITABLE),
                    Some(i) => Some(i | Interests::WRITABLE),
                };
            }
            Ok(Some(r)) => {
                debug!("CLIENT : we wrote {} bytes!", r);
                self.interests = match self.interests {
                    None => Some(Interests::READABLE),
                    Some(i) => Some((i | Interests::READABLE) - Interests::WRITABLE),
                };
            }
            Err(e) => debug!("not implemented; client err={:?}", e),
        }

        if self.interests.unwrap().is_readable() || self.interests.unwrap().is_writable() {
            registry.reregister(
                &self.sock,
                self.token,
                self.interests.unwrap(),
                PollOpt::edge() | PollOpt::oneshot(),
            )?;
        }

        Ok(())
    }

    fn next_msg(&mut self, registry: &Registry) -> io::Result<()> {
        if self.msgs.is_empty() {
            self.shutdown = true;
            return Ok(());
        }

        let curr = self.msgs.remove(0);

        debug!("client prepping next message");
        self.tx = Cursor::new(Bytes::from_static(curr.as_bytes()));
        self.rx = Cursor::new(Bytes::from_static(curr.as_bytes()));

        self.interests = match self.interests {
            None => Some(Interests::WRITABLE),
            Some(i) => Some(i | Interests::WRITABLE),
        };
        registry.reregister(
            &self.sock,
            self.token,
            self.interests.unwrap(),
            PollOpt::edge() | PollOpt::oneshot(),
        )
    }
}

struct Echo {
    server: EchoServer,
    client: EchoClient,
}

impl Echo {
    fn new(srv: TcpListener, client: TcpStream, msgs: Vec<&'static str>) -> Echo {
        Echo {
            server: EchoServer {
                sock: srv,
                conns: Slab::with_capacity(128),
            },
            client: EchoClient::new(client, CLIENT, msgs),
        }
    }
}

#[test]
pub fn test_echo_server() {
    debug!("Starting TEST_ECHO_SERVER");
    let mut poll = Poll::new().unwrap();

    let addr = localhost();
    let srv = TcpListener::bind(&addr).unwrap();

    info!("listen for connections");
    poll.registry()
        .register(
            &srv,
            SERVER,
            Interests::READABLE,
            PollOpt::edge() | PollOpt::oneshot(),
        )
        .unwrap();

    let sock = TcpStream::connect(&addr).unwrap();

    // Connect to the server
    poll.registry()
        .register(
            &sock,
            CLIENT,
            Interests::WRITABLE,
            PollOpt::edge() | PollOpt::oneshot(),
        )
        .unwrap();

    // == Create storage for events
    let mut events = Events::with_capacity(1024);

    let mut handler = Echo::new(srv, sock, vec!["foo", "bar"]);

    // Start the event loop
    while !handler.client.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            debug!("ready {:?} {:?}", event.token(), event.readiness());
            if event.readiness().is_readable() {
                match event.token() {
                    SERVER => handler.server.accept(poll.registry()).unwrap(),
                    CLIENT => handler.client.readable(poll.registry()).unwrap(),
                    i => handler.server.conn_readable(poll.registry(), i).unwrap(),
                }
            }

            if event.readiness().is_writable() {
                match event.token() {
                    SERVER => panic!("received writable for token 0"),
                    CLIENT => handler.client.writable(poll.registry()).unwrap(),
                    i => handler.server.conn_writable(poll.registry(), i).unwrap(),
                };
            }
        }
    }
}
