extern crate nix;
extern crate mio;

use mio::{Reactor, Handler, TcpSocket, SockAddr};

/*
struct Proxy;

impl Handler for Proxy {
    fn accept(token: Token) -> Option<Token> {
        // foo
    }

    fn readable(token);
    fn writable(token);
    fn error(token);
}
*/

struct MyHandler;

impl Handler<()> for MyHandler {
}

pub fn main() {
    println!(" * Initializing reactor");
    let mut reactor = Reactor::<()>::new().unwrap();

    println!(" * Parsing socket address");
    let addr = SockAddr::parse("74.125.28.103:80").expect("could not parse InetAddr");

    println!(" * Creating socket");
    let sock = TcpSocket::v4().unwrap();

    // Configure options

    println!("Connect socket");
    reactor.connect(sock, &addr, ()).unwrap();

    println!("Start reactor");
    reactor.run(MyHandler);

    /*

    // set sock options

    // reactor.connect();
    reactor.run(MyHandler);

    let proxy = Proxy::new();

    let sock = TcpSocket::v4();
    let reactor = Reactor::new();

    reactor.connect(sock, 1);
    reactor.run();
    */
}
