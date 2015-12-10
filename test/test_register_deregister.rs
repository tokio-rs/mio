use mio::*;
use mio::tcp::*;
use bytes::SliceBuf;
use localhost;

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

    fn handle_read(&mut self, event_loop: &mut EventLoop<TestHandler>, token: Token, _: EventSet) {
        match token {
            SERVER => {
                trace!("handle_read; token=SERVER");
                let mut sock = self.server.accept().unwrap().unwrap().0;
                sock.try_write_buf(&mut SliceBuf::wrap("foobar".as_bytes())).unwrap();
            }
            CLIENT => {
                trace!("handle_read; token=CLIENT");
                assert!(self.state == 0, "unexpected state {}", self.state);
                self.state = 1;
                event_loop.reregister(&self.client, CLIENT, EventSet::writable(), PollOpt::level()).unwrap();
            }
            _ => panic!("unexpected token"),
        }
    }

    fn handle_write(&mut self, event_loop: &mut EventLoop<TestHandler>, token: Token, _: EventSet) {
        debug!("handle_write; token={:?}; state={:?}", token, self.state);

        assert!(token == CLIENT, "unexpected token {:?}", token);
        assert!(self.state == 1, "unexpected state {}", self.state);

        self.state = 2;
        event_loop.deregister(&self.client).unwrap();
        event_loop.timeout_ms(1, 200).unwrap();
    }
}

impl Handler for TestHandler {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<TestHandler>, token: Token, events: EventSet) {
        if events.is_readable() {
            self.handle_read(event_loop, token, events);
        }

        if events.is_writable() {
            self.handle_write(event_loop, token, events);
        }
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<TestHandler>, _: usize) {
        trace!("timeout");
        event_loop.shutdown();
    }
}

#[test]
pub fn test_register_deregister() {
    debug!("Starting TEST_REGISTER_DEREGISTER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();

    let server = TcpListener::bind(&addr).unwrap();

    info!("register server socket");
    event_loop.register(&server, SERVER, EventSet::readable(), PollOpt::edge()).unwrap();

    let client = TcpStream::connect(&addr).unwrap();

    // Register client socket only as writable
    event_loop.register(&client, CLIENT, EventSet::readable(), PollOpt::level()).unwrap();

    let mut handler = TestHandler::new(server, client);

    // Start the event loop
    event_loop.run(&mut handler).unwrap();

    assert!(handler.state == 2, "unexpected final state {}", handler.state);
}
