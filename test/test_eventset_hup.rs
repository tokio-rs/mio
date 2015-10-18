use mio::{EventLoop, Handler, Token, EventSet, PollOpt};
use mio::tcp::{TcpListener, TcpStream};
use super::localhost;

struct H { listener: TcpListener }

impl Handler for H {
    type Timeout = ();
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, _events: EventSet) {
        if token == Token(1) {
            let (s, _) = self.listener.accept().unwrap().unwrap();
            event_loop.register(&s, Token(3), EventSet::all(), PollOpt::edge()).unwrap();
            drop(s);
        } else if token == Token(2) {
            event_loop.shutdown();
        }
    }
}

#[test]
pub fn test_eventset_hup() {

    ::env_logger::init().unwrap();
    debug!("Starting TEST_EVENTSET_HUP");

    let addr = localhost();
    let l = TcpListener::bind(&addr).unwrap();
    let s = TcpStream::connect(&addr).unwrap();

    let mut e = EventLoop::new().unwrap();
    e.register(&l, Token(1), EventSet::readable(), PollOpt::edge()).unwrap();
    e.register(&s, Token(2), EventSet::hup(), PollOpt::edge()).unwrap();

    let mut h = H { listener: l };
    e.run(&mut h).unwrap();
}
