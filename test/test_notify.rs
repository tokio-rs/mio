use std::io::timer::sleep;
use std::thread::Thread;
use std::time::Duration;
use mio::*;
use mio::net::*;
use mio::net::tcp::*;
use super::localhost;
use mio::event as evt;

type TestEventLoop = EventLoop<uint, String>;

struct TestHandler {
    sender: EventLoopSender<String>,
    notify: uint
}

impl TestHandler {
    fn new(sender: EventLoopSender<String>) -> TestHandler {
        TestHandler {
            sender: sender,
            notify: 0
        }
    }
}

impl Handler<uint, String> for TestHandler {
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

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    // Setup a server socket so that the event loop blocks
    let srv = TcpSocket::v4().unwrap();
    srv.set_reuseaddr(true).unwrap();

    let srv = srv.bind(&addr).unwrap()
        .listen(256u).unwrap();

    event_loop.register_opt(&srv, Token(0), evt::ALL, evt::EDGE).unwrap();

    let sender = event_loop.channel();

    Thread::spawn(move || {
        sleep(Duration::seconds(1));
        sender.send("First".to_string()).unwrap();
    });

    let sender = event_loop.channel();

    // Start the event loop
    let h = event_loop.run(TestHandler::new(sender))
        .ok().expect("failed to execute event loop");

    assert!(h.notify == 2, "actual={}", h.notify);
}
