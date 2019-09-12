use std::io::{self, Write};
use std::thread::sleep;
use std::time::Duration;

use log::{debug, info, trace};

#[cfg(debug_assertions)]
use mio::net::UdpSocket;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interests, Poll, Registry, Token};

mod util;

use util::{any_local_address, init};

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

struct TestHandler {
    server: TcpListener,
    client: TcpStream,
    state: usize,
}

impl TestHandler {
    fn new(srv: TcpListener, cli: TcpStream) -> TestHandler {
        TestHandler {
            server: srv,
            client: cli,
            state: 0,
        }
    }

    fn handle_read(&mut self, registry: &Registry, token: Token) {
        match token {
            SERVER => {
                trace!("handle_read; token=SERVER");
                let mut sock = self.server.accept().unwrap().0;
                if let Err(err) = sock.write(b"foobar") {
                    if err.kind() != io::ErrorKind::WouldBlock {
                        panic!("unexpected error writing to connection: {}", err);
                    }
                }
            }
            CLIENT => {
                trace!("handle_read; token=CLIENT");
                assert!(self.state == 0, "unexpected state {}", self.state);
                self.state = 1;
                registry
                    .reregister(&self.client, CLIENT, Interests::WRITABLE)
                    .unwrap();
            }
            _ => panic!("unexpected token"),
        }
    }

    fn handle_write(&mut self, registry: &Registry, token: Token) {
        debug!("handle_write; token={:?}; state={:?}", token, self.state);

        assert!(token == CLIENT, "unexpected token {:?}", token);
        assert!(self.state == 1, "unexpected state {}", self.state);

        self.state = 2;
        registry.deregister(&self.client).unwrap();
        registry.deregister(&self.server).unwrap();
    }
}

#[test]
pub fn register_deregister() {
    init();

    debug!("Starting TEST_REGISTER_DEREGISTER");
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(1024);

    let server = TcpListener::bind(any_local_address()).unwrap();
    let addr = server.local_addr().unwrap();

    info!("register server socket");
    poll.registry()
        .register(&server, SERVER, Interests::READABLE)
        .unwrap();

    let client = TcpStream::connect(addr).unwrap();

    // Register client socket only as writable
    poll.registry()
        .register(&client, CLIENT, Interests::READABLE)
        .unwrap();

    let mut handler = TestHandler::new(server, client);

    loop {
        poll.poll(&mut events, None).unwrap();

        if let Some(event) = events.iter().next() {
            if event.is_readable() {
                handler.handle_read(poll.registry(), event.token());
            }

            if event.is_writable() {
                handler.handle_write(poll.registry(), event.token());
                break;
            }
        }
    }

    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
    assert!(events.iter().next().is_none());
}

#[test]
pub fn reregister_different_without_poll() {
    init();

    let mut events = Events::with_capacity(1024);
    let mut poll = Poll::new().unwrap();

    // Create the listener
    let l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.registry()
        .register(&l, Token(0), Interests::READABLE)
        .unwrap();

    let s1 = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    poll.registry()
        .register(&s1, Token(2), Interests::READABLE)
        .unwrap();

    const TIMEOUT: Duration = Duration::from_millis(200);
    sleep(TIMEOUT);

    poll.registry()
        .reregister(&l, Token(0), Interests::WRITABLE)
        .unwrap();

    poll.poll(&mut events, Some(TIMEOUT)).unwrap();
    assert!(events.iter().next().is_none());
}

#[test]
#[cfg(debug_assertions)] // Check is only present when debug assertions are enabled.
fn tcp_register_multiple_event_loops() {
    init();

    let listener = TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();

    let poll1 = Poll::new().unwrap();
    poll1
        .registry()
        .register(
            &listener,
            Token(0),
            Interests::READABLE | Interests::WRITABLE,
        )
        .unwrap();

    let poll2 = Poll::new().unwrap();

    // Try registering the same socket with the initial one
    let res = poll2.registry().register(
        &listener,
        Token(0),
        Interests::READABLE | Interests::WRITABLE,
    );
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);

    // Try cloning the socket and registering it again
    let listener2 = listener.try_clone().unwrap();
    let res = poll2.registry().register(
        &listener2,
        Token(0),
        Interests::READABLE | Interests::WRITABLE,
    );
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);

    // Try the stream
    let stream = TcpStream::connect(addr).unwrap();

    poll1
        .registry()
        .register(&stream, Token(1), Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    let res =
        poll2
            .registry()
            .register(&stream, Token(1), Interests::READABLE | Interests::WRITABLE);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);

    // Try cloning the socket and registering it again
    let stream2 = stream.try_clone().unwrap();
    let res = poll2.registry().register(
        &stream2,
        Token(1),
        Interests::READABLE | Interests::WRITABLE,
    );
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);
}

#[test]
#[cfg(debug_assertions)] // Check is only present when debug assertions are enabled.
fn udp_register_multiple_event_loops() {
    init();

    let socket = UdpSocket::bind(any_local_address()).unwrap();

    let poll1 = Poll::new().unwrap();
    poll1
        .registry()
        .register(&socket, Token(0), Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    let poll2 = Poll::new().unwrap();

    // Try registering the same socket with the initial one
    let res =
        poll2
            .registry()
            .register(&socket, Token(0), Interests::READABLE | Interests::WRITABLE);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);

    // Try cloning the socket and registering it again
    let socket2 = socket.try_clone().unwrap();
    let res = poll2.registry().register(
        &socket2,
        Token(0),
        Interests::READABLE | Interests::WRITABLE,
    );
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::Other);
}

#[test]
fn registering_after_deregistering() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);

    let server = TcpListener::bind(any_local_address()).unwrap();

    poll.registry()
        .register(&server, SERVER, Interests::READABLE)
        .unwrap();

    poll.registry().deregister(&server).unwrap();

    poll.registry()
        .register(&server, SERVER, Interests::READABLE)
        .unwrap();

    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
    assert!(events.is_empty());
}
