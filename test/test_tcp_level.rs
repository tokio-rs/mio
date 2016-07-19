use {sleep_ms, TryRead};
use mio::*;
use mio::tcp::*;
use std::io::Write;
use std::time::Duration;

const MS: u64 = 1_000;

#[test]
pub fn test_tcp_listener_level_triggered() {
    let poll = Poll::new().unwrap();
    let mut pevents = Events::new();

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register(&l, Token(0), EventSet::readable(), PollOpt::level()).unwrap();

    let s1 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();
    poll.register(&s1, Token(1), EventSet::readable(), PollOpt::edge()).unwrap();

    while filter(&pevents, Token(0)).len() == 0 {
        poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    }
    let events = filter(&pevents, Token(0));

    assert_eq!(events.len(), 1);
    assert_eq!(events[0], Event::new(EventSet::readable(), Token(0)));

    poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    let events = filter(&pevents, Token(0));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], Event::new(EventSet::readable(), Token(0)));

    // Accept the connection then test that the events stop
    let _ = l.accept().unwrap();

    poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    let events = filter(&pevents, Token(0));
    assert!(events.is_empty(), "actual={:?}", events);

    let s3 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();
    poll.register(&s3, Token(2), EventSet::readable(), PollOpt::edge()).unwrap();

    while filter(&pevents, Token(0)).len() == 0 {
        poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    }
    let events = filter(&pevents, Token(0));

    assert_eq!(events.len(), 1);
    assert_eq!(events[0], Event::new(EventSet::readable(), Token(0)));

    drop(l);

    poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    let events = filter(&pevents, Token(0));
    assert!(events.is_empty());
}

#[test]
pub fn test_tcp_stream_level_triggered() {
    let poll = Poll::new().unwrap();
    let mut pevents = Events::new();

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register(&l, Token(0), EventSet::readable(), PollOpt::edge()).unwrap();

    let mut s1 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();
    poll.register(&s1, Token(1), EventSet::readable() | EventSet::writable(), PollOpt::level()).unwrap();

    // Sleep a bit to ensure it arrives at dest
    sleep_ms(250);

    let _ = poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    let events: Vec<Event> = (0..pevents.len()).map(|i| pevents.get(i).unwrap()).collect();
    assert!(events.len() == 2, "actual={:?}", events);
    assert_eq!(filter(&pevents, Token(1))[0], Event::new(EventSet::writable(), Token(1)));

    // Server side of socket
    let (mut s1_tx, _) = l.accept().unwrap().unwrap();

    // Sleep a bit to ensure it arrives at dest
    sleep_ms(250);

    poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    let events = filter(&pevents, Token(1));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], Event::new(EventSet::writable(), Token(1)));

    // Register the socket
    poll.register(&s1_tx, Token(123), EventSet::readable(), PollOpt::edge()).unwrap();

    // Write some data
    let res = s1_tx.write(b"hello world!");
    assert!(res.unwrap() > 0);

    // Sleep a bit to ensure it arrives at dest
    sleep_ms(250);

    // Poll rx end
    poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    let events = filter(&pevents, Token(1));
    assert!(events.len() == 1, "actual={:?}", events);
    assert_eq!(events[0], Event::new(EventSet::readable() | EventSet::writable(), Token(1)));

    // Reading the data should clear it
    let mut res = vec![];
    while s1.try_read_buf(&mut res).unwrap().is_some() {
    }

    assert_eq!(res, b"hello world!");

    poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    let events = filter(&pevents, Token(1));
    assert!(events.len() == 1);
    assert_eq!(events[0], Event::new(EventSet::writable(), Token(1)));

    // Closing the socket clears all active level events
    drop(s1);

    poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();
    let events = filter(&pevents, Token(1));
    assert!(events.is_empty());
}

fn filter(events: &Events, token: Token) -> Vec<Event> {
    (0..events.len()).map(|i| events.get(i).unwrap())
                     .filter(|e| e.token() == token)
                     .collect()
}
