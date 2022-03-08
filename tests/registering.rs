#![cfg(not(target_os = "wasi"))]
#![cfg(all(feature = "os-poll", feature = "net"))]

use std::io::{self, Write};
use std::thread::sleep;
use std::time::Duration;

use log::{debug, info, trace};
#[cfg(debug_assertions)]
use mio::net::UdpSocket;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Registry, Token};

mod util;
#[cfg(debug_assertions)]
use util::assert_error;
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
                    .reregister(&mut self.client, CLIENT, Interest::WRITABLE)
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
        registry.deregister(&mut self.client).unwrap();
        registry.deregister(&mut self.server).unwrap();
    }
}

#[test]
pub fn register_deregister() {
    init();

    debug!("Starting TEST_REGISTER_DEREGISTER");
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(1024);

    let mut server = TcpListener::bind(any_local_address()).unwrap();
    let addr = server.local_addr().unwrap();

    info!("register server socket");
    poll.registry()
        .register(&mut server, SERVER, Interest::READABLE)
        .unwrap();

    let mut client = TcpStream::connect(addr).unwrap();

    // Register client socket only as writable
    poll.registry()
        .register(&mut client, CLIENT, Interest::READABLE)
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
pub fn reregister_different_interest_without_poll() {
    init();

    let mut events = Events::with_capacity(1024);
    let mut poll = Poll::new().unwrap();

    // Create the listener
    let mut l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.registry()
        .register(&mut l, Token(0), Interest::READABLE)
        .unwrap();

    let mut s1 = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    poll.registry()
        .register(&mut s1, Token(2), Interest::READABLE)
        .unwrap();

    const TIMEOUT: Duration = Duration::from_millis(200);
    sleep(TIMEOUT);

    poll.registry()
        .reregister(&mut l, Token(0), Interest::WRITABLE)
        .unwrap();

    poll.poll(&mut events, Some(TIMEOUT)).unwrap();
    assert!(events.iter().next().is_none());
}

#[test]
#[cfg(debug_assertions)] // Check is only present when debug assertions are enabled.
fn tcp_register_multiple_event_loops() {
    init();

    let mut listener = TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();

    let poll1 = Poll::new().unwrap();
    poll1
        .registry()
        .register(
            &mut listener,
            Token(0),
            Interest::READABLE | Interest::WRITABLE,
        )
        .unwrap();

    let poll2 = Poll::new().unwrap();

    // Try registering the same socket with the initial one
    let res = poll2.registry().register(
        &mut listener,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    );
    assert_error(res, "I/O source already registered with a `Registry`");

    // Try the stream
    let mut stream = TcpStream::connect(addr).unwrap();

    poll1
        .registry()
        .register(
            &mut stream,
            Token(1),
            Interest::READABLE | Interest::WRITABLE,
        )
        .unwrap();

    let res = poll2.registry().register(
        &mut stream,
        Token(1),
        Interest::READABLE | Interest::WRITABLE,
    );
    assert_error(res, "I/O source already registered with a `Registry`");
}

#[test]
#[cfg(debug_assertions)] // Check is only present when debug assertions are enabled.
fn udp_register_multiple_event_loops() {
    init();

    let mut socket = UdpSocket::bind(any_local_address()).unwrap();

    let poll1 = Poll::new().unwrap();
    poll1
        .registry()
        .register(
            &mut socket,
            Token(0),
            Interest::READABLE | Interest::WRITABLE,
        )
        .unwrap();

    let poll2 = Poll::new().unwrap();

    // Try registering the same socket with the initial one
    let res = poll2.registry().register(
        &mut socket,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    );
    assert_error(res, "I/O source already registered with a `Registry`");
}

#[test]
fn registering_after_deregistering() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);

    let mut server = TcpListener::bind(any_local_address()).unwrap();

    poll.registry()
        .register(&mut server, SERVER, Interest::READABLE)
        .unwrap();

    poll.registry().deregister(&mut server).unwrap();

    poll.registry()
        .register(&mut server, SERVER, Interest::READABLE)
        .unwrap();

    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
    assert!(events.is_empty());
}
