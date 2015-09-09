use mio::*;
use mio::tcp::*;
use mio::util::Slab;
use std::io;
use super::localhost;

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

struct Server {
    sock: TcpListener,
    conns: Slab<TcpStream>
}

impl Server {
    fn accept(&mut self, event_loop: &mut EventLoop<TestHandler>) -> io::Result<()> {
        debug!("server accepting socket");

        let sock = self.sock.accept().unwrap().unwrap();
        let tok = self.conns.insert(sock)
            .ok().expect("could not add socket to slab");

        // Register the connection
        event_loop.register(&self.conns[tok], tok, EventSet::readable(), PollOpt::edge() | PollOpt::oneshot())
            .ok().expect("could not register socket with event loop");

        Ok(())
    }

    fn readable(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token) -> io::Result<()> {
        debug!("server conn readable; tok={:?}", tok);

        let mut buf = [0; 4096];

        let bytes = self.conns[tok].try_read(&mut buf[..]).unwrap();
        debug!("READ={:?}", bytes);

        event_loop.reregister(&self.conns[tok], tok, EventSet::readable(), PollOpt::edge() | PollOpt::oneshot())
    }
}

struct Client {
    sock: TcpStream,
    token: Token,
    msg_count: usize,
}


// Sends a message and expects to receive the same exact message, one at a time
impl Client {
    fn new(sock: TcpStream, tok: Token) -> Client {

        Client {
            sock: sock,
            token: tok,
            msg_count: 0,
        }
    }

    fn writable(&mut self, event_loop: &mut EventLoop<TestHandler>) -> io::Result<()> {
        debug!("client socket writable");

        if self.msg_count > 2 {
            event_loop.shutdown();
        }

        let mut buf = [1];
        let bytes = self.sock.try_write(&mut buf[..]);
        debug!("WROTE={:?} bytes", bytes);
        self.msg_count += 1;
        event_loop.reregister(&self.sock, self.token, EventSet::writable(), PollOpt::edge() | PollOpt::oneshot())
    }
}

struct TestHandler {
    server: Server,
    client: Client,
}

impl TestHandler {
    fn new(srv: TcpListener, client: TcpStream ) -> TestHandler {
        TestHandler {
            server: Server {
                sock: srv,
                conns: Slab::new_starting_at(Token(2), 128)
            },
            client: Client::new(client, CLIENT)
        }
    }
}

impl Handler for TestHandler {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<TestHandler>, token: Token, events: EventSet) {
        if events.is_readable() {
            match token {
                SERVER => self.server.accept(event_loop).unwrap(),
                CLIENT => panic!("received readable for token 1"),
                _ => {
                    self.server.readable(event_loop, token).unwrap();

                    // now that the readable event has reregistered itself, manually deregister it
                    // and remove the connection from the slab
                    event_loop.deregister(&self.server.conns[token]).unwrap();
                    self.server.conns.remove(token);
                }
            }
        }

        if events.is_writable() {
            match token {
                SERVER => panic!("received writable for token 0"),
                CLIENT => self.client.writable(event_loop).unwrap(),
                _ => panic!("received writable for connection")
            };
        }
    }
}

#[test]
pub fn test_deregister_remove() {
    ::env_logger::init().unwrap();
    debug!("Starting TEST_REGISTER_REMOVE");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();
    let srv = TcpListener::bind(&addr).unwrap();

    info!("listen for connections");
    event_loop.register(&srv, SERVER, EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    let sock = TcpStream::connect(&addr).unwrap();

    // Connect to the server
    event_loop.register(&sock, CLIENT, EventSet::writable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    // Start the event loop
    event_loop.run(&mut TestHandler::new(srv, sock)).unwrap();
}
