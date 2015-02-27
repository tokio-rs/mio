use std::old_io::timer::sleep;
use std::thread;
use std::time::Duration;
use mio::*;
use mio::net::*;
use mio::net::tcp::*;
use super::localhost;

type TestEventLoop = EventLoop<usize, String>;

struct TestHandler {
    sender: EventLoopSender<String>,
    notify: usize
}

impl TestHandler {
    fn new(sender: EventLoopSender<String>) -> TestHandler {
        TestHandler {
            sender: sender,
            notify: 0
        }
    }
}

impl Handler<usize, String> for TestHandler {
    fn notify(&mut self, event_loop: &mut TestEventLoop, msg: String) {
        match self.notify {
            0 => {
                assert!(msg.as_slice() == "First", "actual={}", msg);
                self.sender.send("Second".to_string()).unwrap();
            }
            1 => {
                assert!(msg.as_slice() == "Second", "actual={}", msg);
                event_loop.shutdown();
            }
            v => panic!("unexpected value for notify; val={}", v)
        }

        self.notify += 1;
    }
}

#[test]
pub fn test_notify() {
    debug!("Starting TEST_NOTIFY");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();

    // Setup a server socket so that the event loop blocks
    let srv = TcpSocket::v4().unwrap();
    srv.set_reuseaddr(true).unwrap();

    let srv = srv.bind(&addr).unwrap().listen(256).unwrap();

    event_loop.register_opt(&srv, Token(0), Interest::all(), PollOpt::edge()).unwrap();

    let sender = event_loop.channel();

    thread::spawn(move || {
        sleep(Duration::seconds(1));
        sender.send("First".to_string()).unwrap();
    });

    let sender = event_loop.channel();
    let mut handler = TestHandler::new(sender);

    // Start the event loop
    event_loop.run(&mut handler).unwrap();

    assert!(handler.notify == 2, "actual={}", handler.notify);
}
