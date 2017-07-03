use mio::*;
use mio::udp::*;
use sleep_ms;
use std::time::Duration;

const MS: u64 = 1_000;

#[test]
pub fn test_udp_level_triggered() {
    let poll = Poll::new().unwrap();
    let poll = &poll;
    let mut events = Events::with_capacity(1024);
    let events = &mut events;

    // Create the listener
    let tx = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();
    let rx = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    poll.register(&tx, Token(0), Ready::all(), PollOpt::level()).unwrap();
    poll.register(&rx, Token(1), Ready::all(), PollOpt::level()).unwrap();


    for _ in 0..2 {
        expect_events(poll, events, 2, vec![
            Event::new(Ready::writable(), Token(0)),
            Event::new(Ready::writable(), Token(1)),
        ]);
    }

    tx.send_to(b"hello world!", &rx.local_addr().unwrap()).unwrap();

    sleep_ms(250);

    for _ in 0..2 {
        expect_events(poll, events, 2, vec![
            Event::new(Ready::readable() | Ready::writable(), Token(1))
        ]);
    }

    let mut buf = [0; 200];
    while rx.recv_from(&mut buf).unwrap().is_some() {}

    for _ in 0..2 {
        expect_events(poll, events, 4, vec![Event::new(Ready::writable(), Token(1))]);
    }

    tx.send_to(b"hello world!", &rx.local_addr().unwrap()).unwrap();
    sleep_ms(250);

    expect_events(poll, events, 10,
                  vec![Event::new(Ready::readable() | Ready::writable(), Token(1))]);

    drop(rx);
}

fn expect_events(poll: &Poll,
                 event_buffer: &mut Events,
                 poll_try_count: usize,
                 mut expected: Vec<Event>)
{
    for _ in 0..poll_try_count {
        poll.poll(event_buffer, Some(Duration::from_millis(MS))).unwrap();
        for event in event_buffer.iter() {
            remove_item(&mut expected, &event);
        }

        if expected.len() == 0 {
            break;
        }
    }

    assert!(expected.len() == 0, "The following expected events were not found: {:?}", expected);
}

// Temporarily copied from std until stabilization of "vec_remove_item"
fn remove_item<T: PartialEq>(vec: &mut Vec<T>, item: &T) -> Option<T> {
    let pos = match vec.iter().position(|x| *x == *item) {
        Some(x) => x,
        None => return None,
    };
    Some(vec.remove(pos))
}
