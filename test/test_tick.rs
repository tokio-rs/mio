use mio::*;
use std::io::Write;

struct TestHandler {
    tick: usize,
    state: usize,
}

impl TestHandler {
    fn new() -> TestHandler {
        TestHandler {
            tick: 0,
            state: 0,
        }
    }
}

impl Handler for TestHandler {
    type Timeout = usize;
    type Message = String;

    fn tick(&mut self, _event_loop: &mut EventLoop<TestHandler>) {
        debug!("Handler::tick()");
        self.tick += 1;

        assert_eq!(self.state, 1);
        self.state = 0;
    }

    fn ready(&mut self, _event_loop: &mut EventLoop<TestHandler>, token: Token, events: EventSet) {
        if events.is_readable() {
            debug!("Handler::ready() readable event");
            assert_eq!(token, Token(0));
            assert_eq!(self.state, 0);
            self.state = 1;
        }
    }
}

#[test]
pub fn test_tick() {
    debug!("Starting TEST_TICK");
    let mut event_loop = EventLoop::new().ok().expect("Couldn't make event loop");

    let (reader, mut writer) = unix::pipe().unwrap();

    event_loop.register(&reader, Token(0), EventSet::all(),
                        PollOpt::level()).unwrap();

    let mut handler = TestHandler::new();
    writer.write(&[0u8]).unwrap();

    for _ in 0..2 {

        event_loop.run_once(&mut handler).unwrap();
    }

    assert!(handler.tick == 2, "actual={}", handler.tick);
    assert!(handler.state == 0, "actual={}", handler.state);
}
