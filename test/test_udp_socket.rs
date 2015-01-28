use mio::*;
use mio::net::*;
use mio::net::udp::*;
use mio::buf::{RingBuf, SliceBuf};
use std::str;
use super::localhost;
use std::old_io::net::ip::{Ipv4Addr};
use mio::event as evt;

type TestEventLoop = EventLoop<usize, ()>;

const LISTENER: Token = Token(0);
const SENDER: Token = Token(1);

pub struct UdpHandler {
    listen_sock: UdpSocket,
    send_sock: UdpSocket,
    msg: &'static str,
    message_buf: SliceBuf<'static>,
    rx_buf: RingBuf
}

impl UdpHandler {
    fn new(send_sock: UdpSocket, listen_sock: UdpSocket, msg : &'static str) -> UdpHandler {
        UdpHandler {
            listen_sock: listen_sock,
            send_sock: send_sock,
            msg: msg,
            message_buf: SliceBuf::wrap(msg.as_bytes()),
            rx_buf: RingBuf::new(1024)
        }
    }
}

impl Handler<usize, ()> for UdpHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, token: Token, _: evt::ReadHint) {
        match token {
            LISTENER => {
                debug!("We are receiving a datagram now...");
                self.listen_sock.read(&mut self.rx_buf.writer()).unwrap();
                assert!(str::from_utf8(self.rx_buf.reader().bytes()).unwrap() == self.msg);
                event_loop.shutdown();
            },
            _ => ()
        }
    }

    fn writable(&mut self, _: &mut TestEventLoop, token: Token) {
        match token {
            SENDER => {
                self.send_sock.write(&mut self.message_buf).unwrap();
            },
            _ => ()
        }
    }
}

#[test]
pub fn test_udp_socket() {
    debug!("Starting TEST_UDP_SOCKETS");
    let mut event_loop = EventLoop::new().unwrap();

    let send_sock = UdpSocket::v4().unwrap();
    let recv_sock = UdpSocket::v4().unwrap();
    let addr = SockAddr::parse(localhost().as_slice())
        .expect("could not parse InetAddr for localhost");

    info!("Binding both listener and sender to localhost...");
    send_sock.connect(&addr).unwrap();
    recv_sock.bind(&addr).unwrap();

    info!("Setting SO_REUSEADDR");
    send_sock.set_reuseaddr(true).unwrap();
    recv_sock.set_reuseaddr(true).unwrap();

    info!("Joining group 227.1.1.100");
    recv_sock.join_multicast_group(&Ipv4Addr(227, 1, 1, 100), &None).unwrap();

    info!("Joining group 227.1.1.101");
    recv_sock.join_multicast_group(&Ipv4Addr(227, 1, 1, 101), &None).unwrap();

    info!("Registering LISTENER");
    event_loop.register_opt(&recv_sock, LISTENER, evt::READABLE, evt::EDGE).unwrap();

    info!("Registering SENDER");
    event_loop.register_opt(&send_sock, SENDER, evt::WRITABLE, evt::EDGE).unwrap();

    info!("Starting event loop to test with...");
    event_loop.run(UdpHandler::new(send_sock, recv_sock, "hello world")).ok().expect("Failed to run the actual event listener loop");
}

