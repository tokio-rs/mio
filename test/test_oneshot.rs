use mio::*;
use mio::tcp::*;
use std::io::*;
use std::time::Duration;

const MS: u64 = 1_000;

#[test]
pub fn test_tcp_edge_oneshot() {
    let _ = ::env_logger::init();

    let mut poll = Poll::new().unwrap();

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register(&l, Token(0), EventSet::readable(), PollOpt::level()).unwrap();

    // Connect a socket, we are going to write to it
    let mut s1 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();
    poll.register(&s1, Token(1), EventSet::writable(), PollOpt::level()).unwrap();

    wait_for(&mut poll, Token(0));

    // Get pair
    let (mut s2, _) = l.accept().unwrap().unwrap();
    poll.register(&s2, Token(2), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    wait_for(&mut poll, Token(1));

    let res = s1.write(b"foo").unwrap();
    assert_eq!(3, res);

    let mut buf = [0; 1];

    for byte in b"foo" {
        wait_for(&mut poll, Token(2));

        assert_eq!(1, s2.read(&mut buf).unwrap());
        assert_eq!(*byte, buf[0]);

        poll.reregister(&s2, Token(2), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

        if *byte == b'o' {
            poll.reregister(&s2, Token(2), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();
        }
    }
}

fn wait_for(poll: &mut Poll, token: Token) {
    loop {
        println!("~~~~~~ WAITING FOR: {:?}", token);
        poll.poll(Some(Duration::from_millis(MS))).unwrap();

        let cnt = poll.events().filter(|e| e.token() == token).count();

        assert!(cnt < 2, "token appeared multiple times in poll results; cnt={:}", cnt);

        if cnt == 1 { return };
    }
}
