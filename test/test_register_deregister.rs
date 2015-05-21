use mio::*;
use mio::tcp::*;
use super::localhost;

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
}

impl Handler for TestHandler {
    type Timeout = usize;
    type Message = ();

    fn readable(&mut self, event_loop: &mut EventLoop<TestHandler>, token: Token, _: ReadHint) {
        match token {
            SERVER => {
                let mut sock = self.server.accept().unwrap().unwrap();
                sock.try_write_buf(&mut buf::SliceBuf::wrap("foobar".as_bytes())).unwrap();
            }
            CLIENT => {
                assert!(self.state == 0, "unexpected state {}", self.state);
                self.state = 1;
                event_loop.reregister(&self.client, CLIENT, Interest::writable(), PollOpt::level()).unwrap();
            }
            _ => panic!("unexpected token"),
        }
    }

    fn writable(&mut self, event_loop: &mut EventLoop<TestHandler>, token: Token) {
        assert!(token == CLIENT, "unexpected token {:?}", token);
        assert!(self.state == 1, "unexpected state {}", self.state);

        self.state = 2;
        event_loop.deregister(&self.client).unwrap();
        event_loop.timeout_ms(1, 200).unwrap();
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<TestHandler>, _: usize) {
        event_loop.shutdown();
    }
}

#[test]
pub fn test_register_deregister() {
    debug!("Starting TEST_REGISTER_DEREGISTER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();

    let server = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    server.set_reuseaddr(true).unwrap();
    server.bind(&addr).unwrap();

    let server = server.listen(256).unwrap();

    info!("register server socket");
    event_loop.register_opt(&server, SERVER, Interest::readable(), PollOpt::edge()).unwrap();

    let (client, _) = TcpSocket::v4().unwrap()
        .connect(&addr).unwrap();

    // Register client socket only as writable
    event_loop.register_opt(&client, CLIENT, Interest::readable(), PollOpt::level()).unwrap();

    let mut handler = TestHandler::new(server, client);

    // Start the event loop
    event_loop.run(&mut handler).unwrap();

    assert!(handler.state == 2, "unexpected final state {}", handler.state);
}
