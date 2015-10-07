use mio::*;
use mio::unix::*;
use bytes::{Buf, ByteBuf, MutByteBuf, SliceBuf};
use mio::util::Slab;
use std::path::PathBuf;
use std::io;
use std::os::unix::io::AsRawFd;
use tempdir::TempDir;

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

struct EchoConn {
    sock: UnixStream,
    pipe_fd: Option<PipeReader>,
    token: Option<Token>,
    interest: EventSet,
}

impl EchoConn {
    fn new(sock: UnixStream) -> EchoConn {
        EchoConn {
            sock: sock,
            pipe_fd: None,
            token: None,
            interest: EventSet::hup(),
        }
    }

    fn writable(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        let fd = self.pipe_fd.take().unwrap();

        match self.sock.try_write_send_fd(b"x", fd.as_raw_fd()) {
            Ok(None) => {
                debug!("client flushing buf; WOULDBLOCK");

                self.pipe_fd = Some(fd);
                self.interest.insert(EventSet::writable());
            }
            Ok(Some(r)) => {
                debug!("CONN : we wrote {} bytes!", r);

                self.interest.insert(EventSet::readable());
                self.interest.remove(EventSet::writable());
            }
            Err(e) => debug!("not implemented; client err={:?}", e),
        }

        event_loop.reregister(&self.sock, self.token.unwrap(), self.interest, PollOpt::edge() | PollOpt::oneshot())
    }

    fn readable(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        let mut buf = ByteBuf::mut_with_capacity(2048);

        match self.sock.try_read_buf(&mut buf) {
            Ok(None) => {
                panic!("We just got readable, but were unable to read from the socket?");
            }
            Ok(Some(r)) => {
                debug!("CONN : we read {} bytes!", r);
                self.interest.remove(EventSet::readable());
                self.interest.insert(EventSet::writable());
            }
            Err(e) => {
                debug!("not implemented; client err={:?}", e);
                self.interest.remove(EventSet::readable());
            }

        };

        // create fd to pass back. Assume that the write will work
        // without blocking, for simplicity -- we're only testing that
        // the FD makes it through somehow
        let (rd, mut wr) = pipe().unwrap();
        let mut buf = buf.flip();
        match wr.try_write_buf(&mut buf) {
            Ok(None) => {
                panic!("writing to our own pipe blocked :(");
            }
            Ok(Some(r)) => {
                debug!("CONN: we wrote {} bytes to the FD", r);
            }
            Err(e) => {
                panic!("not implemented; client err={:?}", e);
            }
        }
        self.pipe_fd = Some(rd);

        event_loop.reregister(&self.sock, self.token.unwrap(), self.interest, PollOpt::edge() | PollOpt::oneshot())
    }
}

struct EchoServer {
    sock: UnixListener,
    conns: Slab<EchoConn>
}

impl EchoServer {
    fn accept(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        debug!("server accepting socket");

        let sock = self.sock.accept().unwrap().unwrap();
        let conn = EchoConn::new(sock);
        let tok = self.conns.insert(conn)
            .ok().expect("could not add connectiont o slab");

        // Register the connection
        self.conns[tok].token = Some(tok);
        event_loop.register(&self.conns[tok].sock, tok, EventSet::readable(), PollOpt::edge() | PollOpt::oneshot())
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
    sock: UnixStream,
    msgs: Vec<&'static str>,
    tx: SliceBuf<'static>,
    rx: SliceBuf<'static>,
    mut_buf: Option<MutByteBuf>,
    token: Token,
    interest: EventSet,
}


// Sends a message and expects to receive the same exact message, one at a time
impl EchoClient {
    fn new(sock: UnixStream, tok: Token,  mut msgs: Vec<&'static str>) -> EchoClient {
        let curr = msgs.remove(0);

        EchoClient {
            sock: sock,
            msgs: msgs,
            tx: SliceBuf::wrap(curr.as_bytes()),
            rx: SliceBuf::wrap(curr.as_bytes()),
            mut_buf: Some(ByteBuf::mut_with_capacity(2048)),
            token: tok,
            interest: EventSet::none(),
        }
    }

    fn readable(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        debug!("client socket readable");

        let mut buf = self.mut_buf.take().unwrap();
        let mut pipe: PipeReader;

        match self.sock.try_read_buf_recv_fd(&mut buf) {
            Ok(None) => {
                panic!("We just got readable, but were unable to read from the socket?");
            }
            Ok(Some((_, None))) => {
                panic!("Did not receive passed file descriptor");
            }
            Ok(Some((r, Some(fd)))) => {
                debug!("CLIENT : We read {} bytes!", r);
                pipe = From::<Io>::from(From::from(fd));
            }
            Err(e) => {
                panic!("not implemented; client err={:?}", e);
            }
        };

        // read the message accompanying the FD
        let mut buf = buf.flip();
        assert_eq!(buf.remaining(), 1);
        assert_eq!(buf.read_byte(), Some(b'x'));

        // read the data out of the FD itself
        let mut buf = buf.flip();
        match pipe.try_read_buf(&mut buf) {
            Ok(None) => {
                panic!("unimplemented");
            }
            Ok(Some(r)) => {
                debug!("CLIENT : We read {} bytes from the FD", r);
            }
            Err(e) => {
                panic!("not implemented, client err={:?}", e);
            }
        }

        let mut buf = buf.flip();

        debug!("CLIENT : buf = {:?} -- rx = {:?}", buf.bytes(), self.rx.bytes());
        while buf.has_remaining() {
            let actual = buf.read_byte().unwrap();
            let expect = self.rx.read_byte().unwrap();

            assert!(actual == expect, "actual={}; expect={}", actual, expect);
        }

        self.mut_buf = Some(buf.flip());

        self.interest.remove(EventSet::readable());

        if !self.rx.has_remaining() {
            self.next_msg(event_loop).unwrap();
        }

        event_loop.reregister(&self.sock, self.token, self.interest, PollOpt::edge() | PollOpt::oneshot())
    }

    fn writable(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        debug!("client socket writable");

        match self.sock.try_write_buf(&mut self.tx) {
            Ok(None) => {
                debug!("client flushing buf; WOULDBLOCK");
                self.interest.insert(EventSet::writable());
            }
            Ok(Some(r)) => {
                debug!("CLIENT : we wrote {} bytes!", r);
                self.interest.insert(EventSet::readable());
                self.interest.remove(EventSet::writable());
            }
            Err(e) => debug!("not implemented; client err={:?}", e)
        }

        event_loop.reregister(&self.sock, self.token, self.interest, PollOpt::edge() | PollOpt::oneshot())
    }

    fn next_msg(&mut self, event_loop: &mut EventLoop<Echo>) -> io::Result<()> {
        if self.msgs.is_empty() {
            event_loop.shutdown();
            return Ok(());
        }

        let curr = self.msgs.remove(0);

        debug!("client prepping next message");
        self.tx = SliceBuf::wrap(curr.as_bytes());
        self.rx = SliceBuf::wrap(curr.as_bytes());

        self.interest.insert(EventSet::writable());
        event_loop.reregister(&self.sock, self.token, self.interest, PollOpt::edge() | PollOpt::oneshot())
    }
}

struct Echo {
    server: EchoServer,
    client: EchoClient,
}

impl Echo {
    fn new(srv: UnixListener, client: UnixStream, msgs: Vec<&'static str>) -> Echo {
        Echo {
            server: EchoServer {
                sock: srv,
                conns: Slab::new_starting_at(Token(2), 128)
            },
            client: EchoClient::new(client, CLIENT, msgs)
        }
    }
}

impl Handler for Echo {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Echo>, token: Token, events: EventSet) {
        if events.is_readable() {
            match token {
                SERVER => self.server.accept(event_loop).unwrap(),
                CLIENT => self.client.readable(event_loop).unwrap(),
                i => self.server.conn_readable(event_loop, i).unwrap()
            };
        }

        if events.is_writable() {
            match token {
                SERVER => panic!("received writable for token 0"),
                CLIENT => self.client.writable(event_loop).unwrap(),
                _ => self.server.conn_writable(event_loop, token).unwrap()
            };
        }
    }
}

#[test]
pub fn test_unix_pass_fd() {
    debug!("Starting TEST_UNIX_PASS_FD");
    let mut event_loop = EventLoop::new().unwrap();

    let tmp_dir = TempDir::new("test_unix_pass_fd").unwrap();
    let addr = tmp_dir.path().join(&PathBuf::from("sock"));

    let srv = UnixListener::bind(&addr).unwrap();

    info!("listen for connections");
    event_loop.register(&srv, SERVER, EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    let sock = UnixStream::connect(&addr).unwrap();

    // Connect to the server
    event_loop.register(&sock, CLIENT, EventSet::writable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    // Start the event loop
    event_loop.run(&mut Echo::new(srv, sock, vec!["foo", "bar"])).unwrap();
}
