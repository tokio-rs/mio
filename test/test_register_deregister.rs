use mio::*;
use mio::net::*;
use mio::net::tcp::*;
use super::localhost;
use std::time::Duration;

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

type TestEventLoop = EventLoop<usize, ()>;

struct TestHandler {
    server: TcpAcceptor,
    client: TcpSocket,
    state: usize,
}

impl TestHandler {
    fn new(srv: TcpAcceptor, cli: TcpSocket) -> TestHandler {
        TestHandler {
            server: srv,
            client: cli,
            state: 0,
        }
    }
}

impl Handler<usize, ()> for TestHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, token: Token, _: ReadHint) {
        match token {
            SERVER => {
                let sock = self.server.accept().unwrap().unwrap();
                sock.write(&mut buf::SliceBuf::wrap("foobar".as_bytes())).unwrap();
            }
            CLIENT => {
                assert!(self.state == 0, "unexpected state {}", self.state);
                self.state = 1;
                event_loop.reregister(&self.client, CLIENT, Interest::writable(), PollOpt::level()).unwrap();
            }
            _ => panic!("unexpected token"),
        }
    }

    fn writable(&mut self, event_loop: &mut TestEventLoop, token: Token) {
        assert!(token == CLIENT, "unexpected token {:?}", token);
        assert!(self.state == 1, "unexpected state {}", self.state);

        self.state = 2;
        event_loop.deregister(&self.client).unwrap();
        event_loop.timeout(1, Duration::milliseconds(200)).unwrap();
    }

    fn timeout(&mut self, event_loop: &mut TestEventLoop, _: usize) {
        event_loop.shutdown();
    }
}

#[test]
pub fn test_register_deregister() {
    debug!("Starting TEST_REGISTER_DEREGISTER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = SockAddr::parse(localhost().as_slice()).unwrap();

    let server = TcpSocket::v4().unwrap();

    info!("setting re-use addr");
    server.set_reuseaddr(true).unwrap();

    let client = TcpSocket::v4().unwrap();

    // Register client socket only as writable
    event_loop.register_opt(&client, CLIENT, Interest::readable(), PollOpt::level()).unwrap();

    let server = server.bind(&addr).unwrap().listen(256).unwrap();

    info!("register server socket");
    event_loop.register_opt(&server, SERVER, Interest::readable(), PollOpt::edge()).unwrap();

    // Connect to the server
    client.connect(&addr).unwrap();

    let mut handler = TestHandler::new(server, client);

    // Start the event loop
    event_loop.run(&mut handler).unwrap();

    assert!(handler.state == 2, "unexpected final state {}", handler.state);
}
