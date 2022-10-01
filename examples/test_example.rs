fn main() -> Result<(), Box<dyn std::error::Error>> {
    use mio::{Events, Poll, Token, Waker};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    env_logger::init();

    const WAKE_TOKEN: Token = Token(10);
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(2);
    let waker = Arc::new(Waker::new(poll.registry(), WAKE_TOKEN)?);
    // We need to keep the Waker alive, so we'll create a clone for the
    // thread we create below.
    let waker1 = waker.clone();
    let handle = thread::spawn(move || {
        // Working hard, or hardly working?
        thread::sleep(Duration::from_millis(500));
        log::trace!("WAKING!");
        // Now we'll wake the queue on the other thread.
        waker1.wake().expect("unable to wake");
    });
    // On our current thread we'll poll for events, without a timeout.
    poll.poll(&mut events, None)?;
    // After about 500 milliseconds we should be awoken by the other thread and
    // get a single event.
    assert!(!events.is_empty());
    let waker_event = events.iter().next().unwrap();
    assert!(waker_event.is_readable());
    assert_eq!(waker_event.token(), WAKE_TOKEN);
    // We need to tell the waker that we woke up, us otherwise
    // it might wake us again when polling
    log::trace!("Signalling waker it did wake!");
    waker.did_wake();

    log::trace!("About to join thread!");
    handle.join().unwrap();
    log::trace!("Thread joined!");
    Ok(())
}
