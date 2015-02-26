use mio::*;
use mio::net::*;
use mio::net::udp::*;
use mio::buf::{RingBuf, SliceBuf};
use std::str;

type TestEventLoop = EventLoop<usize, ()>;

const LISTENER: Token = Token(0);
const SENDER: Token = Token(1);

pub struct UdpHandler {
    listen_sock: UdpSocket,
    send_sock: UdpSocket,
    sock_addr: SockAddr,
    msg: &'static str,
    message_buf: SliceBuf<'static>,
    rx_buf: RingBuf
}

impl UdpHandler {
    fn new(send_sock: UdpSocket, listen_sock: UdpSocket, msg : &'static str) -> UdpHandler {
        UdpHandler {
            listen_sock: listen_sock,
            send_sock: send_sock,
            sock_addr: SockAddr::parse("127.0.0.1:24601".as_slice()).unwrap(),
            msg: msg,
            message_buf: SliceBuf::wrap(msg.as_bytes()),
            rx_buf: RingBuf::new(1024)
        }
    }
}

impl Handler<usize, ()> for UdpHandler {
    fn readable(&mut self, event_loop: &mut TestEventLoop, token: Token, _: ReadHint) {
        match token {
            LISTENER => {
                debug!("We are receiving a datagram now...");
                match self.listen_sock.recv_from(&mut self.rx_buf.writer()) {
                    Ok(wouldblock) => {
                        match wouldblock.unwrap() {
                            SockAddr::InetAddr(inet) => {
                                assert_eq!(inet.ip(), IpAddr::new_v4(127, 0, 0, 1));
                            }
                            _ => panic!("This should be an IPv4 address")
                        }
                    }
                    ret => {
                        ret.unwrap();
                    }
                }
                assert!(str::from_utf8(self.rx_buf.reader().bytes()).unwrap() == self.msg);
                event_loop.shutdown();
            },
            _ => ()
        }
    }

    fn writable(&mut self, _: &mut TestEventLoop, token: Token) {
        match token {
            SENDER => {
                self.send_sock.send_to(&mut self.message_buf, &self.sock_addr).unwrap();
            },
            _ => ()
        }
    }
}

#[test]
pub fn test_udp_socket_connectionless() {
    debug!("Starting TEST_UDP_CONNECTIONLESS");
    let mut event_loop = EventLoop::new().unwrap();

    let send_sock = UdpSocket::v4().unwrap();
    let recv_sock = UdpSocket::v4().unwrap();
    let addr = SockAddr::parse("127.0.0.1:24601".as_slice()).unwrap();

    info!("Binding the listener socket");
    recv_sock.bind(&addr).unwrap();

    info!("Setting SO_REUSEADDR");
    send_sock.set_reuseaddr(true).unwrap();
    recv_sock.set_reuseaddr(true).unwrap();

    info!("Joining group 227.1.1.100");
    recv_sock.join_multicast_group(&IpAddr::new_v4(227, 1, 1, 100), None).unwrap();

    info!("Joining group 227.1.1.101");
    recv_sock.join_multicast_group(&IpAddr::new_v4(227, 1, 1, 101), None).unwrap();

    info!("Registering LISTENER");
    event_loop.register_opt(&recv_sock, LISTENER, Interest::readable(), PollOpt::edge()).unwrap();

    info!("Registering SENDER");
    event_loop.register_opt(&send_sock, SENDER, Interest::writable(), PollOpt::edge()).unwrap();

    info!("Starting event loop to test with...");
    event_loop.run(&mut UdpHandler::new(send_sock, recv_sock, "hello world")).unwrap();
}
