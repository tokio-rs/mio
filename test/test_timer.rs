use {sleep_ms};
use mio::*;
use mio::timer::{Timer};

use mio::tcp::*;
use bytes::{Buf, ByteBuf, SliceBuf};
use localhost;
use std::time::Duration;

use self::TestState::{Initial, AfterRead, AfterHup};

#[test]
fn test_basic_timer_without_poll() {
    let mut timer = Timer::default();

    // Set the timeout
    timer.set_timeout(Duration::from_millis(200), "hello").unwrap();

    // Nothing when polled immediately
    assert!(timer.poll().is_none());

    // Wait for the timeout
    sleep_ms(200);

    assert_eq!(Some("hello"), timer.poll());
    assert!(timer.poll().is_none());
}

#[test]
fn test_basic_timer_with_poll_edge_set_timeout_after_register() {
    let _ = ::env_logger::init();

    let mut poll = Poll::new().unwrap();
    let mut timer = Timer::default();

    poll.register(&timer, Token(0), EventSet::readable(), PollOpt::edge()).unwrap();
    timer.set_timeout(Duration::from_millis(200), "hello").unwrap();

    let elapsed = elapsed(|| {
        let num = poll.poll(None).unwrap();

        assert_eq!(num, 1);
        assert_eq!(Token(0), poll.events().get(0).unwrap().token());
        assert_eq!(EventSet::readable(), poll.events().get(0).unwrap().kind());
    });

    assert!(is_about(200, elapsed), "actual={:?}", elapsed);
    assert_eq!("hello", timer.poll().unwrap());
    assert_eq!(None, timer.poll());
}

#[test]
fn test_basic_timer_with_poll_edge_set_timeout_before_register() {
    let _ = ::env_logger::init();

    let mut poll = Poll::new().unwrap();
    let mut timer = Timer::default();

    timer.set_timeout(Duration::from_millis(200), "hello").unwrap();
    poll.register(&timer, Token(0), EventSet::readable(), PollOpt::edge()).unwrap();

    let elapsed = elapsed(|| {
        let num = poll.poll(None).unwrap();

        assert_eq!(num, 1);
        assert_eq!(Token(0), poll.events().get(0).unwrap().token());
        assert_eq!(EventSet::readable(), poll.events().get(0).unwrap().kind());
    });

    assert!(is_about(200, elapsed), "actual={:?}", elapsed);
    assert_eq!("hello", timer.poll().unwrap());
    assert_eq!(None, timer.poll());
}

#[test]
fn test_setting_later_timeout_then_earlier_one() {
    let _ = ::env_logger::init();

    let mut poll = Poll::new().unwrap();
    let mut timer = Timer::default();

    poll.register(&timer, Token(0), EventSet::readable(), PollOpt::edge()).unwrap();

    timer.set_timeout(Duration::from_millis(600), "hello").unwrap();
    timer.set_timeout(Duration::from_millis(200), "world").unwrap();

    let elapsed = elapsed(|| {
        let num = poll.poll(None).unwrap();

        assert_eq!(num, 1);
        assert_eq!(Token(0), poll.events().get(0).unwrap().token());
        assert_eq!(EventSet::readable(), poll.events().get(0).unwrap().kind());
    });

    assert!(is_about(200, elapsed), "actual={:?}", elapsed);
    assert_eq!("world", timer.poll().unwrap());
    assert_eq!(None, timer.poll());

    let elapsed = self::elapsed(|| {
        let num = poll.poll(None).unwrap();

        assert_eq!(num, 1);
        assert_eq!(Token(0), poll.events().get(0).unwrap().token());
        assert_eq!(EventSet::readable(), poll.events().get(0).unwrap().kind());
    });

    assert!(is_about(400, elapsed), "actual={:?}", elapsed);
    assert_eq!("hello", timer.poll().unwrap());
    assert_eq!(None, timer.poll());
}

#[test]
fn test_timer_with_looping_wheel() {
    let _ = ::env_logger::init();

    let mut poll = Poll::new().unwrap();
    let mut timer = timer::Builder::default()
        .num_slots(2)
        .build();

    poll.register(&timer, Token(0), EventSet::readable(), PollOpt::edge()).unwrap();

    const TOKENS: &'static [ &'static str ] = &[ "hello", "world", "some", "thing" ];

    for (i, msg) in TOKENS.iter().enumerate() {
        timer.set_timeout(Duration::from_millis(500 * (i as u64 + 1)), msg).unwrap();
    }

    for msg in TOKENS {
        let elapsed = elapsed(|| {
            let num = poll.poll(None).unwrap();

            assert_eq!(num, 1);
            assert_eq!(Token(0), poll.events().get(0).unwrap().token());
            assert_eq!(EventSet::readable(), poll.events().get(0).unwrap().kind());
        });

        assert!(is_about(500, elapsed), "actual={:?}; msg={:?}", elapsed, msg);
        assert_eq!(Some(msg), timer.poll());
        assert_eq!(None, timer.poll());

    }
}

#[test]
fn test_edge_without_polling() {
    let _ = ::env_logger::init();

    let mut poll = Poll::new().unwrap();
    let mut timer = Timer::default();

    poll.register(&timer, Token(0), EventSet::readable(), PollOpt::edge()).unwrap();

    timer.set_timeout(Duration::from_millis(400), "hello").unwrap();

    let ms = elapsed(|| {
        let num = poll.poll(None).unwrap();
        assert_eq!(num, 1);
        assert_eq!(Token(0), poll.events().get(0).unwrap().token());
        assert_eq!(EventSet::readable(), poll.events().get(0).unwrap().kind());
    });

    assert!(is_about(400, ms), "actual={:?}", ms);

    let ms = elapsed(|| {
        let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
        assert_eq!(num, 0);
    });

    assert!(is_about(300, ms), "actual={:?}", ms);
}

#[test]
fn test_level_triggered() {
    let _ = ::env_logger::init();

    let mut poll = Poll::new().unwrap();
    let mut timer = Timer::default();

    poll.register(&timer, Token(0), EventSet::readable(), PollOpt::level()).unwrap();

    timer.set_timeout(Duration::from_millis(400), "hello").unwrap();

    let ms = elapsed(|| {
        let num = poll.poll(None).unwrap();
        assert_eq!(num, 1);
        assert_eq!(Token(0), poll.events().get(0).unwrap().token());
        assert_eq!(EventSet::readable(), poll.events().get(0).unwrap().kind());
    });

    assert!(is_about(400, ms), "actual={:?}", ms);

    let ms = elapsed(|| {
        let num = poll.poll(None).unwrap();
        assert_eq!(num, 1);
        assert_eq!(Token(0), poll.events().get(0).unwrap().token());
        assert_eq!(EventSet::readable(), poll.events().get(0).unwrap().kind());
    });

    assert!(is_about(0, ms), "actual={:?}", ms);
}

#[test]
fn test_edge_oneshot_triggered() {
    let _ = ::env_logger::init();

    let mut poll = Poll::new().unwrap();
    let mut timer = Timer::default();

    poll.register(&timer, Token(0), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    timer.set_timeout(Duration::from_millis(200), "hello").unwrap();

    let ms = elapsed(|| {
        let num = poll.poll(None).unwrap();
        assert_eq!(num, 1);
    });

    assert!(is_about(200, ms), "actual={:?}", ms);

    let ms = elapsed(|| {
        let num = poll.poll(Some(Duration::from_millis(300))).unwrap();
        assert_eq!(num, 0);
    });

    assert!(is_about(300, ms), "actual={:?}", ms);

    poll.reregister(&timer, Token(0), EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

    let ms = elapsed(|| {
        let num = poll.poll(None).unwrap();
        assert_eq!(num, 1);
    });

    assert!(is_about(0, ms));
}

fn elapsed<F: FnMut()>(mut f: F) -> u64 {
    use std::time::Instant;

    let now = Instant::now();

    f();

    let elapsed = now.elapsed();
    elapsed.as_secs() * 1000 + (elapsed.subsec_nanos() / 1_000_000) as u64
}

fn is_about(expect: u64, val: u64) -> bool {
    const WINDOW: i64 = 200;

    ((expect as i64) - (val as i64)).abs() <= WINDOW
}

/*
 *
 * ===== OLD TIMER =====
 *
 */

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);
const CONN: Token = Token(2);

#[derive(Debug, PartialEq)]
enum TestState {
    Initial,
    AfterRead,
    AfterHup
}

struct TestHandler {
    srv: TcpListener,
    cli: TcpStream,
    state: TestState
}

impl TestHandler {
    fn new(srv: TcpListener, cli: TcpStream) -> TestHandler {
        TestHandler {
            srv: srv,
            cli: cli,
            state: Initial
        }
    }

    fn handle_read(&mut self, event_loop: &mut EventLoop<TestHandler>,
                   tok: Token, events: EventSet) {
        match tok {
            SERVER => {
                debug!("server connection ready for accept");
                let conn = self.srv.accept().unwrap().unwrap().0;
                event_loop.register(&conn, CONN, EventSet::all(),
                                        PollOpt::edge()).unwrap();
                event_loop.timeout(conn, Duration::from_millis(200)).unwrap();

                event_loop.reregister(&self.srv, SERVER, EventSet::readable(),
                                      PollOpt::edge()).unwrap();
            }
            CLIENT => {
                debug!("client readable");

                match self.state {
                    Initial => {
                        // Whether or not Hup is included with actual data is
                        // platform specific
                        if events.is_hup() {
                            self.state = AfterHup;
                        } else {
                            self.state = AfterRead;
                        }
                    }
                    AfterRead => {
                        assert_eq!(events, EventSet::readable() | EventSet::hup());
                        self.state = AfterHup;
                    }
                    AfterHup => panic!("Shouldn't get here"),
                }

                if self.state == AfterHup {
                    event_loop.shutdown();
                    return;
                }

                let mut buf = ByteBuf::mut_with_capacity(2048);

                match self.cli.try_read_buf(&mut buf) {
                    Ok(n) => {
                        debug!("read {:?} bytes", n);
                        assert!(b"zomg" == buf.flip().bytes());
                    }
                    Err(e) => {
                        debug!("client sock failed to read; err={:?}", e.kind());
                    }
                }

                event_loop.reregister(&self.cli, CLIENT,
                                      EventSet::readable() | EventSet::hup(),
                                      PollOpt::edge()).unwrap();
            }
            CONN => {}
            _ => panic!("received unknown token {:?}", tok),
        }
    }

    fn handle_write(&mut self, event_loop: &mut EventLoop<TestHandler>,
                    tok: Token, _: EventSet) {
        match tok {
            SERVER => panic!("received writable for token 0"),
            CLIENT => debug!("client connected"),
            CONN => {}
            _ => panic!("received unknown token {:?}", tok),
        }

        event_loop.reregister(&self.cli, CLIENT, EventSet::readable(),
                              PollOpt::edge()).unwrap();
    }
}

impl Handler for TestHandler {
    type Timeout = TcpStream;
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<TestHandler>, tok: Token, events: EventSet) {
        if events.is_readable() {
            self.handle_read(event_loop, tok, events);
        }

        if events.is_writable() {
            self.handle_write(event_loop, tok, events);
        }
    }

    fn timeout(&mut self, _event_loop: &mut EventLoop<TestHandler>, mut sock: TcpStream) {
        debug!("timeout handler : writing to socket");
        sock.try_write_buf(&mut SliceBuf::wrap(b"zomg")).unwrap().unwrap();
    }
}

#[test]
pub fn test_old_timer() {
    let _ = ::env_logger::init();

    debug!("Starting TEST_TIMER");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();

    let srv = TcpListener::bind(&addr).unwrap();

    info!("listening for connections");

    event_loop.register(&srv, SERVER, EventSet::all(), PollOpt::edge()).unwrap();

    let sock = TcpStream::connect(&addr).unwrap();

    // Connect to the server
    event_loop.register(&sock, CLIENT, EventSet::all(), PollOpt::edge()).unwrap();

    // Init the handler
    let mut handler = TestHandler::new(srv, sock);
    // Start the event loop
    event_loop.run(&mut handler).unwrap();

    assert!(handler.state == AfterHup, "actual={:?}", handler.state);
}
