use mio::*;
use mio::udp::*;
use bytes::{Buf, RingBuf, SliceBuf, MutBuf};
use std::str;
use localhost;

const LISTENER: Token = Token(0);
const SENDER: Token = Token(1);

pub struct UdpHandler {
    tx: UdpSocket,
    rx: UdpSocket,
    msg: &'static str,
    buf: SliceBuf<'static>,
    rx_buf: RingBuf
}

impl UdpHandler {
    fn new(tx: UdpSocket, rx: UdpSocket, msg : &'static str) -> UdpHandler {
        UdpHandler {
            tx: tx,
            rx: rx,
            msg: msg,
            buf: SliceBuf::wrap(msg.as_bytes()),
            rx_buf: RingBuf::new(1024)
        }
    }
}

impl Handler for UdpHandler {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<UdpHandler>, token: Token, events: EventSet) {

        if events.is_readable() {
            match token {
                LISTENER => {
                    debug!("We are receiving a datagram now...");
                    let (cnt, _) = unsafe {
                        self.rx.recv_from(self.rx_buf.mut_bytes()).unwrap()
                                                                  .unwrap()
                    };
                    unsafe { MutBuf::advance(&mut self.rx_buf, cnt); }
                    assert!(str::from_utf8(self.rx_buf.bytes()).unwrap() == self.msg);
                    event_loop.shutdown();
                },
                _ => ()
            }
        }

        if events.is_writable() {
            match token {
                SENDER => {
                    let addr = self.rx.local_addr().unwrap();
                    let cnt = self.tx.send_to(self.buf.bytes(), &addr).unwrap()
                                                                      .unwrap();
                    self.buf.advance(cnt);
                },
                _ => {}
            }
        }
    }
}

fn assert_send<T: Send>() {
}

#[test]
pub fn test_udp_socket() {
    debug!("Starting TEST_UDP_SOCKETS");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();
    let any = str::FromStr::from_str("0.0.0.0:0").unwrap();

    let tx = UdpSocket::bound(&any).unwrap();
    let rx = UdpSocket::bound(&addr).unwrap();

    assert_send::<UdpSocket>();

    // ensure that the sockets are non-blocking
    let mut buf = [0; 128];
    assert!(rx.recv_from(&mut buf).unwrap().is_none());

    info!("Registering SENDER");
    event_loop.register(&tx, SENDER, EventSet::writable(), PollOpt::edge()).unwrap();

    info!("Registering LISTENER");
    event_loop.register(&rx, LISTENER, EventSet::readable(), PollOpt::edge()).unwrap();

    info!("Starting event loop to test with...");
    event_loop.run(&mut UdpHandler::new(tx, rx, "hello world")).unwrap();
}
