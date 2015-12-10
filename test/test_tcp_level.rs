use mio::*;
use mio::tcp::*;
use std::io::Write;
use sleep_ms;

const MS: usize = 1_000;

#[test]
pub fn test_tcp_listener_level_triggered() {
    let mut poll = Poll::new().unwrap();

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register(&l, Token(0), EventSet::readable(), PollOpt::level()).unwrap();

    let s1 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();
    poll.register(&s1, Token(1), EventSet::readable(), PollOpt::edge()).unwrap();

    poll.poll(Some(MS)).unwrap();
    let events = filter(&poll, Token(0));

    assert_eq!(events.len(), 1);
    assert_eq!(events[0], IoEvent::new(EventSet::readable(), Token(0)));

    poll.poll(Some(MS)).unwrap();
    let events = filter(&poll, Token(0));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], IoEvent::new(EventSet::readable(), Token(0)));

    // Accept the connection then test that the events stop
    let _ = l.accept().unwrap();

    poll.poll(Some(MS)).unwrap();
    let events = filter(&poll, Token(0));
    assert!(events.is_empty(), "actual={:?}", events);

    let s3 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();
    poll.register(&s3, Token(2), EventSet::readable(), PollOpt::edge()).unwrap();

    poll.poll(Some(MS)).unwrap();
    let events = filter(&poll, Token(0));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], IoEvent::new(EventSet::readable(), Token(0)));

    drop(l);

    poll.poll(Some(MS)).unwrap();
    let events = filter(&poll, Token(0));
    assert!(events.is_empty());
}

#[test]
pub fn test_tcp_stream_level_triggered() {
    let mut poll = Poll::new().unwrap();

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register(&l, Token(0), EventSet::readable(), PollOpt::edge()).unwrap();

    let mut s1 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();
    poll.register(&s1, Token(1), EventSet::readable() | EventSet::writable(), PollOpt::level()).unwrap();

    let _ = poll.poll(Some(MS)).unwrap();
    let events: Vec<IoEvent> = poll.events().collect();
    assert!(events.len() == 2, "actual={:?}", events);
    assert_eq!(filter(&poll, Token(1))[0], IoEvent::new(EventSet::writable(), Token(1)));

    // Server side of socket
    let (mut s1_tx, _) = l.accept().unwrap().unwrap();

    poll.poll(Some(MS)).unwrap();
    let events = filter(&poll, Token(1));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], IoEvent::new(EventSet::writable(), Token(1)));

    // Register the socket
    poll.register(&s1_tx, Token(123), EventSet::readable(), PollOpt::edge()).unwrap();

    // Write some data
    let res = s1_tx.write(b"hello world!");
    assert!(res.unwrap() > 0);

    // Sleep a bit to ensure it arrives at dest
    sleep_ms(250);

    // Poll rx end
    poll.poll(Some(MS)).unwrap();
    let events = filter(&poll, Token(1));
    assert!(events.len() == 1, "actual={:?}", events);
    assert_eq!(events[0], IoEvent::new(EventSet::readable() | EventSet::writable(), Token(1)));

    // Reading the data should clear it
    let mut res = vec![];
    while s1.try_read_buf(&mut res).unwrap().is_some() {
    }

    assert_eq!(res, b"hello world!");

    poll.poll(Some(MS)).unwrap();
    let events = filter(&poll, Token(1));
    assert!(events.len() == 1);
    assert_eq!(events[0], IoEvent::new(EventSet::writable(), Token(1)));

    // Closing the socket clears all active level events
    drop(s1);

    poll.poll(Some(MS)).unwrap();
    let events = filter(&poll, Token(1));
    assert!(events.is_empty());
}

fn filter(poll: &Poll, token: Token) -> Vec<IoEvent> {
    poll.events().filter(|e| e.token == token).collect()
}
