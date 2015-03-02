use mio::*;
use mio::net::*;
use mio::net::udp::*;
use mio::buf::{RingBuf, SliceBuf};
use super::localhost;
use std::str;
use std::net::{SocketAddr, IpAddr};

type TestEventLoop = EventLoop<usize, ()>;

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

impl Handler<usize, ()> for UdpHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, token: Token, _: ReadHint) {
        match token {
            LISTENER => {
                debug!("We are receiving a datagram now...");
                TryRecv::recv_from(&self.rx, &mut self.rx_buf.writer()).unwrap();
                assert!(str::from_utf8(self.rx_buf.reader().bytes()).unwrap() == self.msg);
                event_loop.shutdown();
            },
            _ => ()
        }
    }

    fn writable(&mut self, _: &mut TestEventLoop, token: Token) {
        match token {
            SENDER => {
                TrySend::send_to(&self.tx, &mut self.buf, &self.rx.socket_addr().unwrap()).unwrap();
            },
            _ => {}
        }
    }
}

#[test]
pub fn test_udp_socket() {
    debug!("Starting TEST_UDP_SOCKETS");
    let mut event_loop = EventLoop::new().unwrap();

    let addr = localhost();
    let any = SocketAddr::new(IpAddr::new_v4(0, 0, 0, 0), 0);

    let tx = UdpSocket::bind(&any).unwrap();
    let rx = UdpSocket::bind(&addr).unwrap();

    info!("Registering SENDER");
    event_loop.register_opt(&tx, SENDER, Interest::writable(), PollOpt::edge()).unwrap();

    info!("Registering LISTENER");
    event_loop.register_opt(&rx, LISTENER, Interest::readable(), PollOpt::edge()).unwrap();

    info!("Starting event loop to test with...");
    event_loop.run(&mut UdpHandler::new(tx, rx, "hello world")).unwrap();
}
