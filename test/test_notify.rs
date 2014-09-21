use std::io::timer::sleep;
use std::time::Duration;
use mio::*;
use super::localhost;

type TestReactor = Reactor<uint, String>;

struct TestHandler {
    sender: ReactorSender<String>,
    notify: uint
}

impl TestHandler {
    fn new(sender: ReactorSender<String>) -> TestHandler {
        TestHandler {
            sender: sender,
            notify: 0
        }
    }
}

impl Handler<uint, String> for TestHandler {
    fn notify(&mut self, reactor: &mut TestReactor, msg: String) {
        match self.notify {
            0 => {
                assert!(msg.as_slice() == "First", "actual={}", msg);
                self.sender.send("Second".to_string()).unwrap();
            }
            1 => {
                assert!(msg.as_slice() == "Second", "actual={}", msg);
                reactor.shutdown();
            }
            v => fail!("unexpected value for notify; val={}", v)
        }

        self.notify += 1;
    }
}

#[test]
pub fn test_notify() {
    let mut reactor = Reactor::new().unwrap();

    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr");

    // Setup a server socket so that the reactor blocks
    let srv = TcpSocket::v4().unwrap();
    srv.set_reuseaddr(true).unwrap();
    let srv = srv.bind(&addr).unwrap();
    reactor.listen(&srv, 256u, 0u).unwrap();

    let sender = reactor.channel();

    spawn(proc() {
        sleep(Duration::seconds(1));
        sender.send("First".to_string()).unwrap();
    });

    let sender = reactor.channel();

    // Start the reactor
    let h = reactor.run(TestHandler::new(sender))
        .ok().expect("failed to execute reactor");

    assert!(h.notify == 2, "actual={}", h.notify);
}
