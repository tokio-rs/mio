#![cfg(unix)]

use mio::*;
use std::time::Duration;

pub struct BrokenPipeHandler;

impl Handler for BrokenPipeHandler {
    type Timeout = ();
    type Message = ();
    fn ready(&mut self, _: &mut EventLoop<Self>, token: Token, _: EventSet) {
        if token == Token(1) {
            panic!("Received ready() on a closed pipe.");
        }
    }
}

#[test]
pub fn broken_pipe() {
    let mut event_loop: EventLoop<BrokenPipeHandler> = EventLoop::new().unwrap();
    let (reader, _) = unix::pipe().unwrap();

    // On Darwin this returns a "broken pipe" error.
    let _ = event_loop.register(&reader, Token(1), EventSet::all(), PollOpt::edge());

    let mut handler = BrokenPipeHandler;
    drop(reader);
    event_loop.run_once(&mut handler, Some(Duration::from_millis(1000))).unwrap();
}
