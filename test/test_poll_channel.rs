use {sleep_ms};
use mio::*;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[test]
pub fn test_poll_channel_edge() {
    let mut poll = Poll::new().unwrap();
    let (tx, rx) = channel::from_std_channel(mpsc::channel());

    poll.register(&rx, Token(123), EventSet::readable(), PollOpt::edge()).unwrap();

    // Wait, but nothing should happen
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    // Push the value
    tx.send("hello").unwrap();

    // Polling will contain the event
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(1, num);

    let event = poll.events().get(0).unwrap();
    assert_eq!(event.token(), Token(123));
    assert_eq!(event.kind(), EventSet::readable());

    // Poll again and there should be no events
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    // Read the value
    assert_eq!("hello", rx.try_recv().unwrap());

    // Poll again, nothing
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    // Push a value
    tx.send("goodbye").unwrap();

    // Have an event
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(1, num);

    let event = poll.events().get(0).unwrap();
    assert_eq!(event.token(), Token(123));
    assert_eq!(event.kind(), EventSet::readable());
}

#[test]
pub fn test_poll_channel_oneshot() {
    let mut poll = Poll::new().unwrap();
    let (tx, rx) = channel::from_std_channel(mpsc::channel());

    poll.register(&rx, Token(123), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    // Wait, but nothing should happen
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    // Push the value
    tx.send("hello").unwrap();

    // Polling will contain the event
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(1, num);

    let event = poll.events().get(0).unwrap();
    assert_eq!(event.token(), Token(123));
    assert_eq!(event.kind(), EventSet::readable());

    // Poll again and there should be no events
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    // Read the value
    assert_eq!("hello", rx.try_recv().unwrap());

    // Poll again, nothing
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    // Push a value
    tx.send("goodbye").unwrap();

    // Poll again, nothing
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    // Reregistering will re-trigger the notification
    for _ in 0..3 {
        poll.reregister(&rx, Token(123), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

        // Have an event
        let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
        assert_eq!(1, num);

        let event = poll.events().get(0).unwrap();
        assert_eq!(event.token(), Token(123));
        assert_eq!(event.kind(), EventSet::readable());
    }

    // Get the value
    assert_eq!("goodbye", rx.try_recv().unwrap());

    poll.reregister(&rx, Token(123), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    // Have an event
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    poll.reregister(&rx, Token(123), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    // Have an event
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);
}

#[test]
pub fn test_poll_channel_level() {
    let mut poll = Poll::new().unwrap();
    let (tx, rx) = channel::from_std_channel(mpsc::channel());

    poll.register(&rx, Token(123), EventSet::readable(), PollOpt::level()).unwrap();

    // Wait, but nothing should happen
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    // Push the value
    tx.send("hello").unwrap();

    // Polling will contain the event
    for i in 0..5 {
        let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
        assert!(1 == num, "actually got {} on iteration {}", num, i);

        let event = poll.events().get(0).unwrap();
        assert_eq!(event.token(), Token(123));
        assert_eq!(event.kind(), EventSet::readable());
    }

    // Read the value
    assert_eq!("hello", rx.try_recv().unwrap());

    // Wait, but nothing should happen
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);
}

#[test]
pub fn test_poll_channel_writable() {
    let mut poll = Poll::new().unwrap();
    let (tx, rx) = channel::from_std_channel(mpsc::channel());

    poll.register(&rx, Token(123), EventSet::writable(), PollOpt::edge()).unwrap();

    // Wait, but nothing should happen
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);

    // Push the value
    tx.send("hello").unwrap();

    // Wait, but nothing should happen
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);
}

#[test]
pub fn test_dropping_receive_before_poll() {
    let mut poll = Poll::new().unwrap();
    let (tx, rx) = channel::from_std_channel(mpsc::channel());

    poll.register(&rx, Token(123), EventSet::readable(), PollOpt::edge()).unwrap();

    // Push the value
    tx.send("hello").unwrap();

    // Drop the receive end
    drop(rx);

    // Wait, but nothing should happen
    let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
    assert_eq!(0, num);
}

#[test]
pub fn test_mixing_channel_with_socket() {
    use mio::tcp::*;

    let mut poll = Poll::new().unwrap();
    let (tx, rx) = channel::from_std_channel(mpsc::channel());

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register(&l, Token(0), EventSet::readable(), PollOpt::edge()).unwrap();
    poll.register(&rx, Token(1), EventSet::readable(), PollOpt::edge()).unwrap();

    // Push a value onto the channel
    tx.send("hello").unwrap();

    // Connect a TCP socket
    let s1 = TcpStream::connect(&l.local_addr().unwrap()).unwrap();

    // Register the socket
    poll.register(&s1, Token(2), EventSet::readable(), PollOpt::edge()).unwrap();

    // Sleep a bit to ensure it arrives at dest
    sleep_ms(250);

    poll.poll(Some(Duration::from_millis(300))).unwrap();
    let events = filter(&poll, Token(0));

    assert_eq!(1, events.len());

    let events = filter(&poll, Token(1));
    assert_eq!(1, events.len());
}

#[test]
pub fn test_sending_from_other_thread_while_polling() {
    const ITERATIONS: usize = 20;
    const THREADS: usize = 5;

    // Make sure to run multiple times
    let mut poll = Poll::new().unwrap();

    for _ in 0..ITERATIONS {
        let (tx, rx) = channel::from_std_channel(mpsc::channel());
        poll.register(&rx, Token(0), EventSet::readable(), PollOpt::edge()).unwrap();

        for _ in 0..THREADS {
            let tx = tx.clone();

            thread::spawn(move || {
                sleep_ms(50);
                tx.send("ping").unwrap();
            });
        }

        let mut recv = 0;

        while recv < THREADS {
            let num = poll.poll(None).unwrap();

            if num != 0 {
                assert_eq!(1, num);
                assert_eq!(poll.events().get(0).unwrap().token(), Token(0));

                while let Ok(_) = rx.try_recv() {
                    recv += 1;
                }
            }
        }
    }
}

fn filter(poll: &Poll, token: Token) -> Vec<Event> {
    poll.events().filter(|e| e.token() == token).collect()
}
