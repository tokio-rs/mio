use std::io::{self, Read};
use std::time::Duration;
use std::{net, thread};

use mio::net::TcpStream;
use mio::{Events, Interests, Poll, Token};

mod util;

use util::init;

#[test]
fn issue_776() {
    init();

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().expect("accept").0;
        s.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set_read_timeout");
        let _ = s.read(&mut [0; 16]).expect("read");
    });

    let mut poll = Poll::new().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, Token(1), Interests::READABLE | Interests::WRITABLE)
        .unwrap();
    let mut events = Events::with_capacity(16);
    'outer: loop {
        poll.poll(&mut events, None).unwrap();
        for event in &events {
            if event.token() == Token(1) {
                // connected
                break 'outer;
            }
        }
    }

    let mut b = [0; 1024];
    match s.read(&mut b) {
        Ok(_) => panic!("unexpected ok"),
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => (),
        Err(e) => panic!("unexpected error: {:?}", e),
    }

    drop(s);
    t.join().unwrap();
}
