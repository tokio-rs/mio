use mio::*;
use mio::udp::*;
use sleep_ms;
use std::time::Duration;

const MS: u64 = 1_000;

#[test]
pub fn test_udp_level_triggered() {
    let mut poll = Poll::new().unwrap();

    // Create the listener
    let tx = UdpSocket::bound(&"127.0.0.1:0".parse().unwrap()).unwrap();
    let rx = UdpSocket::bound(&"127.0.0.1:0".parse().unwrap()).unwrap();

    poll.register(&tx, Token(0), EventSet::all(), PollOpt::level()).unwrap();
    poll.register(&rx, Token(1), EventSet::all(), PollOpt::level()).unwrap();

    for _ in 0..2 {
        poll.poll(Some(Duration::from_millis(MS))).unwrap();

        let tx_events = filter(&poll, Token(0));
        assert_eq!(1, tx_events.len());
        assert_eq!(tx_events[0], Event::new(EventSet::writable(), Token(0)));

        let rx_events = filter(&poll, Token(1));
        assert_eq!(1, rx_events.len());
        assert_eq!(rx_events[0], Event::new(EventSet::writable(), Token(1)));
    }

    tx.send_to(b"hello world!", &rx.local_addr().unwrap()).unwrap();

    sleep_ms(250);

    for _ in 0..2 {
        poll.poll(Some(Duration::from_millis(MS))).unwrap();
        let rx_events = filter(&poll, Token(1));
        assert_eq!(1, rx_events.len());
        assert_eq!(rx_events[0], Event::new(EventSet::readable() | EventSet::writable(), Token(1)));
    }

    let mut buf = [0; 200];
    while rx.recv_from(&mut buf).unwrap().is_some() {
    }

    for _ in 0..2 {
        poll.poll(Some(Duration::from_millis(MS))).unwrap();
        let rx_events = filter(&poll, Token(1));
        assert_eq!(1, rx_events.len());
        assert_eq!(rx_events[0], Event::new(EventSet::writable(), Token(1)));
    }

    tx.send_to(b"hello world!", &rx.local_addr().unwrap()).unwrap();
    sleep_ms(250);

    poll.poll(Some(Duration::from_millis(MS))).unwrap();
    let rx_events = filter(&poll, Token(1));
    assert_eq!(1, rx_events.len());
    assert_eq!(rx_events[0], Event::new(EventSet::readable() | EventSet::writable(), Token(1)));

    drop(rx);

    poll.poll(Some(Duration::from_millis(MS))).unwrap();
    let rx_events = filter(&poll, Token(1));
    assert!(rx_events.is_empty());
}

fn filter(poll: &Poll, token: Token) -> Vec<Event> {
    poll.events().filter(|e| e.token == token).collect()
}
