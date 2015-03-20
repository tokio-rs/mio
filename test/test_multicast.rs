use mio::*;
use mio::udp::*;
use mio::buf::{RingBuf, SliceBuf};
use std::str;
use std::net::{SocketAddr, Ipv4Addr};
use super::localhost;

const LISTENER: Token = Token(0);
const SENDER: Token = Token(1);

pub struct UdpHandler {
    tx: NonBlock<UdpSocket>,
    rx: NonBlock<UdpSocket>,
    msg: &'static str,
    buf: SliceBuf<'static>,
    rx_buf: RingBuf
}

impl UdpHandler {
    fn new(tx: NonBlock<UdpSocket>, rx: NonBlock<UdpSocket>, msg: &'static str) -> UdpHandler {
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

    fn readable(&mut self, event_loop: &mut EventLoop<UdpHandler>, token: Token, _: ReadHint) {
        match token {
            LISTENER => {
                debug!("We are receiving a datagram now...");
                match self.rx.recv_from(&mut self.rx_buf) {
                    Ok(Some(SocketAddr::V4(addr))) => {
                        assert_eq!(*addr.ip(), Ipv4Addr::new(127, 0, 0, 1));
                    }
                    _ => panic!("unexpected result"),
                }
                assert!(str::from_utf8(self.rx_buf.bytes()).unwrap() == self.msg);
                event_loop.shutdown();
            },
            _ => ()
        }
    }

    fn writable(&mut self, _: &mut EventLoop<UdpHandler>, token: Token) {
        match token {
            SENDER => {
                self.tx.send_to(&mut self.buf, &self.rx.local_addr().unwrap()).unwrap();
            },
            _ => ()
        }
    }
}

#[test]
pub fn test_multicast() {
    debug!("Starting TEST_UDP_CONNECTIONLESS");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();
    let any = "0.0.0.0:0".parse().unwrap();

    let tx = udp::bind(&any).unwrap();
    let rx = udp::bind(&addr).unwrap();

    info!("Joining group 227.1.1.100");
    rx.join_multicast(&"227.1.1.100:0".parse().unwrap()).unwrap();

    info!("Joining group 227.1.1.101");
    rx.join_multicast(&"227.1.1.101:0".parse().unwrap()).unwrap();

    info!("Registering SENDER");
    event_loop.register_opt(&tx, SENDER, Interest::writable(), PollOpt::edge()).unwrap();

    info!("Registering LISTENER");
    event_loop.register_opt(&rx, LISTENER, Interest::readable(), PollOpt::edge()).unwrap();

    info!("Starting event loop to test with...");
    event_loop.run(&mut UdpHandler::new(tx, rx, "hello world")).unwrap();
}
