#![feature(globs)]

extern crate nix;
extern crate mio;

mod client {
    use std::str;
    use mio::*;

    struct MyHandler {
        sock: TcpSocket,
        done: bool
    }

    impl MyHandler {
        pub fn new(sock: TcpSocket) -> MyHandler {
            MyHandler {
                sock: sock,
                done: false
            }
        }
    }

    impl Handler<uint> for MyHandler {
        fn readable(&mut self, reactor: &mut Reactor, tok: uint) {
            let mut buf = Vec::from_fn(1024, |_| 0);
            let mut i = 0u;

            loop {
                i += 1;

                match self.sock.read(buf.as_mut_slice()) {
                    Ok(cnt) => {
                        println!("{}", str::from_utf8(buf.as_slice().slice_to(cnt)));
                    }
                    Err(e) if e.is_eof() => {
                        println!("EOF");
                        return;
                    }
                    e => {
                        println!("error: {}", e);
                        return;
                    }
                }
            }
        }

        fn writable(&mut self, reactor: &mut Reactor, tok: uint) {
            if self.done {
                return;
            }

            println!("Connected, writing payload");

            self.done = true;
            self.sock.write(
                b"GET / HTTP/1.1\r\n\
                  Connection: keep-alive\r\n\
                  Host: localhost\r\n\r\n").unwrap();
        }
    }

    pub fn run() {
        println!(" * Initializing reactor");
        let mut reactor = Reactor::<uint>::new().unwrap();

        println!(" * Parsing socket address");
        let addr = SockAddr::parse("127.0.0.1:9292").expect("could not parse InetAddr");

        println!(" * Creating socket");
        let sock = TcpSocket::v4().unwrap();

        // Configure options

        println!("Connect socket");
        reactor.connect(sock, &addr, 123u).unwrap();

        println!("Start reactor");
        reactor.run(MyHandler::new(sock));
    }
}

mod server {
    use std::str;
    use mio::*;

    struct MyHandler {
        srv: TcpAcceptor,
        socks: Vec<TcpSocket>
    }

    impl MyHandler {
        fn new(srv: TcpAcceptor) -> MyHandler {
            MyHandler {
                srv: srv,
                socks: vec![]
            }
        }
    }

    impl Handler<uint> for MyHandler {
        fn readable(&mut self, reactor: &mut Reactor, tok: uint) {
            match tok {
                0 => {
                    println!("Accepting socket");
                    let sock = self.srv.accept().unwrap();

                    let i = self.socks.len();
                    self.socks.push(sock);

                    reactor.register(sock, i + 1);
                }
                i => {
                    let sock = self.socks.get_mut(i - 1);
                    let mut buf = Vec::from_fn(1024, |_| 0);

                    loop {
                        match sock.read(buf.as_mut_slice()) {
                            Ok(cnt) => {
                                println!("{}", str::from_utf8(buf.as_slice().slice_to(cnt)));
                            }
                            Err(e) if e.is_eof() => {
                                println!("EOF");
                                return;
                            }
                            Err(e) if e.is_would_block() => {
                                println!("WouldBlock");
                                return;
                            }
                            e => {
                                println!("error: {}", e);
                                return;
                            }
                        }
                    }
                }
            }
        }

        fn writable(&mut self, reactor: &mut Reactor, tok: uint) {
            println!("Writable; tok={}", tok);
        }
    }

    pub fn run() {
        println!(" * Initializing reactor");
        let mut reactor = Reactor::<uint>::new().unwrap();

        let addr = SockAddr::parse("127.0.0.1:8080")
            .expect("could not parse InetAddr");

        println!(" * Create socket");
        let sock = TcpSocket::v4().unwrap()
            .bind(&addr).unwrap();

        println!(" * Listening");
        reactor.listen(sock, 256u, 0u);

        println!(" * Start reactor");
        reactor.run(MyHandler::new(sock));
    }
}

pub fn main() {
    // client::run();
    server::run();
}
