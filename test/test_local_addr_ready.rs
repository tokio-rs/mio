use {TryWrite};
use mio::*;
use mio::deprecated::{EventLoop, Handler};
use mio::net::{TcpListener, TcpStream};

const LISTEN: Token = Token(0);
const CLIENT: Token = Token(1);
const SERVER: Token = Token(2);

struct MyHandler {
    listener: TcpListener,
    connected: TcpStream,
    accepted: Option<TcpStream>,
}

impl Handler for MyHandler {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self,
             event_loop: &mut EventLoop<MyHandler>,
             token: Token,
             _: Ready) {
        match token {
            LISTEN => {
                let sock = self.listener.accept().unwrap().0;
                event_loop.register(&sock,
                                    SERVER,
                                    Ready::writable(),
                                    PollOpt::edge()).unwrap();
                self.accepted = Some(sock);
            }
            SERVER => {
                self.accepted.as_ref().unwrap().peer_addr().unwrap();
                self.accepted.as_ref().unwrap().local_addr().unwrap();
                self.accepted.as_mut().unwrap().try_write(&[1, 2, 3]).unwrap();
                self.accepted = None;
            }
            CLIENT => {
                self.connected.peer_addr().unwrap();
                self.connected.local_addr().unwrap();
                event_loop.shutdown();
            }
            _ => panic!("unexpected token"),
        }
    }
}

#[test]
fn local_addr_ready() {
    let addr = "127.0.0.1:0".parse().unwrap();
    let server = TcpListener::bind(&addr).unwrap();
    let addr = server.local_addr().unwrap();

    let mut event_loop = EventLoop::new().unwrap();
    event_loop.register(&server, LISTEN, Ready::readable(),
                        PollOpt::edge()).unwrap();

    let sock = TcpStream::connect(&addr).unwrap();
    event_loop.register(&sock, CLIENT, Ready::readable(),
                        PollOpt::edge()).unwrap();

    event_loop.run(&mut MyHandler {
        listener: server,
        connected: sock,
        accepted: None,
    }).unwrap();
}
