extern crate nix;
extern crate mio;

use std::mem;
use nix::sys::utsname::uname;
use mio::{Reactor, Handler, IoHandle, TcpSocket, SockAddr};

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
    println!("ZOMG; {}", uname().release());

    let mut reactor = Reactor::<()>::new().unwrap();
    let addr = SockAddr::parse("74.125.28.103:80").expect("could not parse InetAddr");
    let sock = TcpSocket::v4().unwrap();

    // Configure options

    reactor.connect(sock, addr, ()).unwrap();
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
