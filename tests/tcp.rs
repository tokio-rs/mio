use std::io::{self, Read, Write};
use std::net::Shutdown;
#[cfg(unix)]
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::sync::mpsc::channel;
use std::time::Duration;
use std::{net, thread};

use bytes::{Buf, Bytes, BytesMut};
use log::{debug, info};
#[cfg(unix)]
use net2::TcpStreamExt;
use slab::Slab;

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interests, Poll, Registry, Token};

mod util;

use util::{any_local_address, assert_send, assert_sync, init, TryRead, TryWrite};

const LISTEN: Token = Token(0);
const CLIENT: Token = Token(1);
const SERVER: Token = Token(2);

#[test]
fn is_send_and_sync() {
    assert_send::<TcpListener>();
    assert_sync::<TcpListener>();

    assert_send::<TcpStream>();
    assert_sync::<TcpStream>();
}

#[test]
fn accept() {
    init();

    struct H {
        hit: bool,
        listener: TcpListener,
        shutdown: bool,
    }

    let l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        net::TcpStream::connect(addr).unwrap();
    });

    let mut poll = Poll::new().unwrap();

    poll.registry()
        .register(&l, Token(1), Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        hit: false,
        listener: l,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            h.hit = true;
            assert_eq!(event.token(), Token(1));
            assert!(event.is_readable());
            assert!(h.listener.accept().is_ok());
            h.shutdown = true;
        }
    }
    assert!(h.hit);
    assert!(h.listener.accept().unwrap_err().kind() == io::ErrorKind::WouldBlock);
    t.join().unwrap();
}

#[test]
fn connect() {
    init();

    struct H {
        hit: u32,
        shutdown: bool,
    }

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();

    let (tx, rx) = channel();
    let (tx2, rx2) = channel();
    let t = thread::spawn(move || {
        let s = l.accept().unwrap();
        rx.recv().unwrap();
        drop(s);
        tx2.send(()).unwrap();
    });

    let mut poll = Poll::new().unwrap();
    let s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, Token(1), Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        hit: 0,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), Token(1));
            match h.hit {
                0 => assert!(event.is_writable()),
                1 => assert!(event.is_readable()),
                _ => panic!(),
            }
            h.hit += 1;
            h.shutdown = true;
        }
    }
    assert_eq!(h.hit, 1);
    tx.send(()).unwrap();
    rx2.recv().unwrap();
    h.shutdown = false;
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), Token(1));
            match h.hit {
                0 => assert!(event.is_writable()),
                1 => assert!(event.is_readable()),
                _ => panic!(),
            }
            h.hit += 1;
            h.shutdown = true;
        }
    }
    assert_eq!(h.hit, 2);
    t.join().unwrap();
}

#[test]
fn read() {
    init();

    const N: usize = 16 * 1024 * 1024;
    struct H {
        amt: usize,
        socket: TcpStream,
        shutdown: bool,
    }

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let b = [0; 1024];
        let mut amt = 0;
        while amt < N {
            amt += s.write(&b).unwrap();
        }
    });

    let mut poll = Poll::new().unwrap();
    let s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, Token(1), Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        amt: 0,
        socket: s,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), Token(1));
            let mut b = [0; 1024];
            loop {
                if let Some(amt) = h.socket.try_read(&mut b).unwrap() {
                    h.amt += amt;
                } else {
                    break;
                }
                if h.amt >= N {
                    h.shutdown = true;
                    break;
                }
            }
        }
    }
    t.join().unwrap();
}

#[test]
fn peek() {
    init();

    const N: usize = 16 * 1024 * 1024;
    struct H {
        amt: usize,
        socket: TcpStream,
        shutdown: bool,
    }

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let b = [0; 1024];
        let mut amt = 0;
        while amt < N {
            amt += s.write(&b).unwrap();
        }
    });

    let mut poll = Poll::new().unwrap();
    let s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, Token(1), Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        amt: 0,
        socket: s,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), Token(1));
            let mut b = [0; 1024];
            match h.socket.peek(&mut b) {
                Ok(_) => (),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => panic!("unexpected error: {:?}", e),
            }

            loop {
                if let Some(amt) = h.socket.try_read(&mut b).unwrap() {
                    h.amt += amt;
                } else {
                    break;
                }
                if h.amt >= N {
                    h.shutdown = true;
                    break;
                }
            }
        }
    }
    t.join().unwrap();
}

#[test]
fn write() {
    init();

    const N: usize = 16 * 1024 * 1024;
    struct H {
        amt: usize,
        socket: TcpStream,
        shutdown: bool,
    }

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let mut b = [0; 1024];
        let mut amt = 0;
        while amt < N {
            amt += s.read(&mut b).unwrap();
        }
    });

    let mut poll = Poll::new().unwrap();
    let s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, Token(1), Interests::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        amt: 0,
        socket: s,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), Token(1));
            let b = [0; 1024];
            loop {
                if let Some(amt) = h.socket.try_write(&b).unwrap() {
                    h.amt += amt;
                } else {
                    break;
                }
                if h.amt >= N {
                    h.shutdown = true;
                    break;
                }
            }
        }
    }
    t.join().unwrap();
}

#[test]
fn connect_then_close() {
    init();

    struct H {
        listener: TcpListener,
        shutdown: bool,
    }

    let mut poll = Poll::new().unwrap();
    let l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let s = TcpStream::connect(l.local_addr().unwrap()).unwrap();

    poll.registry()
        .register(&l, Token(1), Interests::READABLE)
        .unwrap();
    poll.registry()
        .register(&s, Token(2), Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        listener: l,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(1) {
                let s = h.listener.accept().unwrap().0;
                poll.registry()
                    .register(&s, Token(3), Interests::READABLE | Interests::WRITABLE)
                    .unwrap();
                drop(s);
            } else if event.token() == Token(2) {
                h.shutdown = true;
            }
        }
    }
}

#[test]
fn listen_then_close() {
    init();

    let mut poll = Poll::new().unwrap();
    let l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();

    poll.registry()
        .register(&l, Token(1), Interests::READABLE)
        .unwrap();
    drop(l);

    let mut events = Events::with_capacity(128);

    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();

    for event in &events {
        if event.token() == Token(1) {
            panic!("recieved ready() on a closed TcpListener")
        }
    }
}

#[test]
fn bind_twice_bad() {
    init();

    let l1 = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = l1.local_addr().unwrap();
    assert!(TcpListener::bind(addr).is_err());
}

#[test]
fn multiple_writes_immediate_success() {
    init();

    const N: usize = 16;
    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let mut b = [0; 1024];
        let mut amt = 0;
        while amt < 1024 * N {
            for byte in b.iter_mut() {
                *byte = 0;
            }
            let n = s.read(&mut b).unwrap();
            amt += n;
            for byte in b[..n].iter() {
                assert_eq!(*byte, 1);
            }
        }
    });

    let mut poll = Poll::new().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&s, Token(1), Interests::WRITABLE)
        .unwrap();
    let mut events = Events::with_capacity(16);

    // Wait for our TCP stream to connect
    'outer: loop {
        poll.poll(&mut events, None).unwrap();
        for event in events.iter() {
            if event.token() == Token(1) && event.is_writable() {
                break 'outer;
            }
        }
    }

    for _ in 0..N {
        s.write_all(&[1; 1024]).unwrap();
    }

    t.join().unwrap();
}

#[test]
#[cfg(unix)]
fn connection_reset_by_peer() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);
    let mut buf = [0u8; 16];

    // Create listener
    let l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = l.local_addr().unwrap();

    // Connect client
    let client = net2::TcpBuilder::new_v4().unwrap().to_tcp_stream().unwrap();

    client.set_linger(Some(Duration::from_millis(0))).unwrap();
    client.connect(&addr).unwrap();

    // Convert to Mio stream
    // FIXME: how to convert the stream on Windows?
    let client = unsafe { TcpStream::from_raw_fd(client.into_raw_fd()) };

    // Register server
    poll.registry()
        .register(&l, Token(0), Interests::READABLE)
        .unwrap();

    // Register interest in the client
    poll.registry()
        .register(&client, Token(1), Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    // Wait for listener to be ready
    let mut server;
    'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(0) {
                match l.accept() {
                    Ok((sock, _)) => {
                        server = sock;
                        break 'outer;
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => panic!("unexpected error {:?}", e),
                }
            }
        }
    }

    // Close the connection
    drop(client);

    // Wait a moment
    thread::sleep(Duration::from_millis(100));

    // Register interest in the server socket
    poll.registry()
        .register(&server, Token(3), Interests::READABLE)
        .unwrap();

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(3) {
                assert!(event.is_readable());

                match server.read(&mut buf) {
                    Ok(0) | Err(_) => {}

                    Ok(x) => panic!("expected empty buffer but read {} bytes", x),
                }
                return;
            }
        }
    }
}

#[test]
fn connect_error() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);

    // Pick a "random" port that shouldn't be in use.
    let l = match TcpStream::connect("127.0.0.1:38381".parse().unwrap()) {
        Ok(l) => l,
        Err(ref e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            // Connection failed synchronously.  This is not a bug, but it
            // unfortunately doesn't get us the code coverage we want.
            return;
        }
        Err(e) => panic!("TcpStream::connect unexpected error {:?}", e),
    };

    poll.registry()
        .register(&l, Token(0), Interests::WRITABLE)
        .unwrap();

    'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(0) {
                assert!(event.is_writable());
                break 'outer;
            }
        }
    }

    assert!(l.take_error().unwrap().is_some());
}

#[test]
fn write_error() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);
    let (tx, rx) = channel();

    let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let t = thread::spawn(move || {
        let (conn, _addr) = listener.accept().unwrap();
        rx.recv().unwrap();
        drop(conn);
    });

    let mut s = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&s, Token(0), Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    let mut wait_writable = || 'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(0) && event.is_writable() {
                break 'outer;
            }
        }
    };

    wait_writable();

    tx.send(()).unwrap();
    t.join().unwrap();

    let buf = [0; 1024];
    loop {
        match s.write(&buf) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => wait_writable(),
            Err(e) => {
                println!("good error: {}", e);
                break;
            }
        }
    }
}

macro_rules! wait {
    ($poll:ident, $ready:ident, $expect_hup: expr) => {{
        use std::time::Instant;

        let now = Instant::now();
        let mut events = Events::with_capacity(16);
        let mut found = false;

        while !found {
            if now.elapsed() > Duration::from_secs(5) {
                panic!("not ready");
            }

            $poll
                .poll(&mut events, Some(Duration::from_secs(1)))
                .unwrap();

            for event in &events {
                // Hup is only generated on kqueue platforms.
                #[cfg(any(
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "ios",
                    target_os = "macos",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                {
                    if $expect_hup {
                        assert!(event.is_read_hup());
                    }
                }

                if !$expect_hup {
                    assert!(!event.is_hup());
                }

                if event.token() == Token(0) && event.$ready() {
                    found = true;
                    break;
                }
            }
        }
    }};
}

#[test]
fn write_shutdown() {
    init();

    let mut poll = Poll::new().unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let interests = Interests::READABLE | Interests::WRITABLE;

    let client = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&client, Token(0), interests)
        .unwrap();

    let (socket, _) = listener.accept().unwrap();

    wait!(poll, is_writable, false);

    let mut events = Events::with_capacity(16);

    // Polling should not have any events
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();

    let next = events.iter().next();
    assert!(next.is_none());

    println!("SHUTTING DOWN");
    // Now, shutdown the write half of the socket.
    socket.shutdown(Shutdown::Write).unwrap();

    wait!(poll, is_readable, true);
}

struct MyHandler {
    listener: TcpListener,
    connected: TcpStream,
    accepted: Option<TcpStream>,
    shutdown: bool,
}

#[test]
fn local_addr_ready() {
    init();

    let addr = "127.0.0.1:0".parse().unwrap();
    let server = TcpListener::bind(addr).unwrap();
    let addr = server.local_addr().unwrap();

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&server, LISTEN, Interests::READABLE)
        .unwrap();

    let sock = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&sock, CLIENT, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);

    let mut handler = MyHandler {
        listener: server,
        connected: sock,
        accepted: None,
        shutdown: false,
    };

    while !handler.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            match event.token() {
                LISTEN => {
                    let sock = handler.listener.accept().unwrap().0;
                    poll.registry()
                        .register(&sock, SERVER, Interests::WRITABLE)
                        .unwrap();
                    handler.accepted = Some(sock);
                }
                SERVER => {
                    handler.accepted.as_ref().unwrap().peer_addr().unwrap();
                    handler.accepted.as_ref().unwrap().local_addr().unwrap();
                    handler
                        .accepted
                        .as_mut()
                        .unwrap()
                        .try_write(&[1, 2, 3])
                        .unwrap();
                    handler.accepted = None;
                }
                CLIENT => {
                    handler.connected.peer_addr().unwrap();
                    handler.connected.local_addr().unwrap();
                    handler.shutdown = true;
                }
                _ => panic!("unexpected token"),
            }
        }
    }
}

struct EchoConn {
    sock: TcpStream,
    buf: Option<Bytes>,
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

                self.mut_buf = Some(buf.try_mut().unwrap());

                self.interests = Some(Interests::READABLE);
            }
            Err(e) => debug!("not implemented; client err={:?}", e),
        }

        assert!(
            self.interests.unwrap().is_readable() || self.interests.unwrap().is_writable(),
            "actual={:?}",
            self.interests
        );
        registry.reregister(&self.sock, self.token.unwrap(), self.interests.unwrap())
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
                self.buf = Some(buf.freeze());

                self.interests = Some(Interests::WRITABLE);
            }
            Err(e) => {
                debug!("not implemented; client err={:?}", e);
                if self.interests == Some(Interests::READABLE) {
                    self.interests = None;
                } else if let Some(x) = self.interests.as_mut() {
                    *x = Interests::WRITABLE;
                }
            }
        };

        assert!(
            self.interests.unwrap().is_readable() || self.interests.unwrap().is_writable(),
            "actual={:?}",
            self.interests
        );
        registry.reregister(&self.sock, self.token.unwrap(), self.interests.unwrap())
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
            .register(&self.conns[tok].sock, Token(tok), Interests::READABLE)
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
    tx: Bytes,
    rx: Bytes,
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
            tx: Bytes::from_static(curr.as_bytes()),
            rx: Bytes::from_static(curr.as_bytes()),
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
                let mut buf = buf.freeze();

                while buf.has_remaining() {
                    let actual = buf.get_u8();
                    let expect = self.rx.get_u8();

                    assert!(actual == expect, "actual={}; expect={}", actual, expect);
                }

                self.mut_buf = Some(buf.try_mut().unwrap());

                if self.interests == Some(Interests::READABLE) {
                    self.interests = None;
                } else if let Some(x) = self.interests.as_mut() {
                    *x = Interests::WRITABLE;
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
            registry.reregister(&self.sock, self.token, x)?;
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
                    Some(_) => Some(Interests::READABLE),
                };
            }
            Err(e) => debug!("not implemented; client err={:?}", e),
        }

        if self.interests.unwrap().is_readable() || self.interests.unwrap().is_writable() {
            registry.reregister(&self.sock, self.token, self.interests.unwrap())?;
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
        self.tx = Bytes::from_static(curr.as_bytes());
        self.rx = Bytes::from_static(curr.as_bytes());

        self.interests = match self.interests {
            None => Some(Interests::WRITABLE),
            Some(i) => Some(i | Interests::WRITABLE),
        };
        registry.reregister(&self.sock, self.token, self.interests.unwrap())
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
pub fn echo_server() {
    init();

    debug!("Starting TEST_ECHO_SERVER");
    let mut poll = Poll::new().unwrap();

    let srv = TcpListener::bind(any_local_address()).unwrap();
    let addr = srv.local_addr().unwrap();

    info!("listen for connections");
    poll.registry()
        .register(&srv, SERVER, Interests::READABLE)
        .unwrap();

    let sock = TcpStream::connect(addr).unwrap();

    // Connect to the server
    poll.registry()
        .register(&sock, CLIENT, Interests::WRITABLE)
        .unwrap();

    // == Create storage for events
    let mut events = Events::with_capacity(1024);

    let mut handler = Echo::new(srv, sock, vec!["foo", "bar"]);

    // Start the event loop
    while !handler.client.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            debug!("ready {:?} {:?}", event.token(), event);
            if event.is_readable() {
                match event.token() {
                    SERVER => handler.server.accept(poll.registry()).unwrap(),
                    CLIENT => handler.client.readable(poll.registry()).unwrap(),
                    i => handler.server.conn_readable(poll.registry(), i).unwrap(),
                }
            }

            if event.is_writable() {
                match event.token() {
                    SERVER => panic!("received writable for token 0"),
                    CLIENT => handler.client.writable(poll.registry()).unwrap(),
                    i => handler.server.conn_writable(poll.registry(), i).unwrap(),
                };
            }
        }
    }
}

#[test]
fn write_then_drop() {
    init();

    let a = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = a.local_addr().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();

    let mut poll = Poll::new().unwrap();

    poll.registry()
        .register(&a, Token(1), Interests::READABLE)
        .unwrap();

    poll.registry()
        .register(&s, Token(3), Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(1));

    let mut s2 = a.accept().unwrap().0;

    poll.registry()
        .register(&s2, Token(2), Interests::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(2));

    s2.write_all(&[1, 2, 3, 4]).unwrap();
    drop(s2);

    poll.registry()
        .reregister(&s, Token(3), Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(3));

    let mut buf = [0; 10];
    assert_eq!(s.read(&mut buf).unwrap(), 4);
    assert_eq!(&buf[0..4], &[1, 2, 3, 4]);
}

#[test]
fn write_then_deregister() {
    init();

    let a = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = a.local_addr().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();

    let mut poll = Poll::new().unwrap();

    poll.registry()
        .register(&a, Token(1), Interests::READABLE)
        .unwrap();
    poll.registry()
        .register(&s, Token(3), Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(1));

    let mut s2 = a.accept().unwrap().0;

    poll.registry()
        .register(&s2, Token(2), Interests::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(2));

    s2.write_all(&[1, 2, 3, 4]).unwrap();
    poll.registry().deregister(&s2).unwrap();

    poll.registry()
        .reregister(&s, Token(3), Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(3));

    let mut buf = [0; 10];
    assert_eq!(s.read(&mut buf).unwrap(), 4);
    assert_eq!(&buf[0..4], &[1, 2, 3, 4]);
}
