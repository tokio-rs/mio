use mio::*;
use mio::deprecated::{Handler, EventLoop};
use mio::udp::*;
use bytes::{Buf, RingBuf, SliceBuf, MutBuf};
use std::str;
use std::time;
use localhost;

const LISTENER: Token = Token(0);
const SENDER: Token = Token(1);

pub struct UdpHandlerSendRecv {
    tx: UdpSocket,
    rx: UdpSocket,
    msg: &'static str,
    buf: SliceBuf<'static>,
    rx_buf: RingBuf,
    connected: bool
}

impl UdpHandlerSendRecv {
    fn new(tx: UdpSocket, rx: UdpSocket, connected: bool, msg : &'static str) -> UdpHandlerSendRecv {
        UdpHandlerSendRecv {
            tx: tx,
            rx: rx,
            msg: msg,
            buf: SliceBuf::wrap(msg.as_bytes()),
            rx_buf: RingBuf::new(1024),
            connected: connected
        }
    }
}

impl Handler for UdpHandlerSendRecv {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, event_loop: &mut EventLoop<UdpHandlerSendRecv>, token: Token, events: Ready) {

        if events.is_readable() {
            match token {
                LISTENER => {
                    debug!("We are receiving a datagram now...");
                    let cnt = unsafe {
                        if !self.connected {
                            self.rx.recv_from(self.rx_buf.mut_bytes()).unwrap()
                                                                      .unwrap().0
                        } else {
                            self.rx.recv(self.rx_buf.mut_bytes()).unwrap()
                                                                    .unwrap()
                        }
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
                    let cnt = if !self.connected {
                        let addr = self.rx.local_addr().unwrap();
                        self.tx.send_to(self.buf.bytes(), &addr).unwrap()
                                                                .unwrap()
                    } else {
                        self.tx.send(self.buf.bytes()).unwrap()
                                                      .unwrap()
                    };

                    self.buf.advance(cnt);
                },
                _ => {}
            }
        }
    }
}

pub struct UdpHandlerTimeout {
}

impl Handler for UdpHandlerTimeout {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, _event_loop: &mut EventLoop<UdpHandlerTimeout>, token: Token, events: Ready) {
        if events.is_readable() {
            match token {
                LISTENER => {
                    assert!(false, "Expected to no receive a packet but got something")
                },
                _ => ()
            }
        }
   }

    /// Invoked when a timeout has completed.
    fn timeout(&mut self, event_loop: &mut EventLoop<Self>, _timeout: Self::Timeout) {
        event_loop.shutdown();
    }
}

fn assert_send<T: Send>() {
}

fn assert_sync<T: Sync>() {
}

#[cfg(test)]
fn test_send_recv_udp(tx: UdpSocket, rx: UdpSocket, connected: bool) {
    debug!("Starting TEST_UDP_SOCKETS");
    let mut event_loop = EventLoop::new().unwrap();

    assert_send::<UdpSocket>();
    assert_sync::<UdpSocket>();

    // ensure that the sockets are non-blocking
    let mut buf = [0; 128];
    assert!(rx.recv_from(&mut buf).unwrap().is_none());

    info!("Registering SENDER");
    event_loop.register(&tx, SENDER, Ready::writable(), PollOpt::edge()).unwrap();

    info!("Registering LISTENER");
    event_loop.register(&rx, LISTENER, Ready::readable(), PollOpt::edge()).unwrap();

    info!("Starting event loop to test with...");
    event_loop.run(&mut UdpHandlerSendRecv::new(tx, rx, connected, "hello world")).unwrap();
}

#[test]
pub fn test_udp_socket() {
    let addr = localhost();
    let any = localhost();

    let tx = UdpSocket::bind(&any).unwrap();
    let rx = UdpSocket::bind(&addr).unwrap();

    test_send_recv_udp(tx, rx, false);
}

#[test]
pub fn test_udp_socket_send_recv() {
    let addr = localhost();
    let any = localhost();

    let tx = UdpSocket::bind(&any).unwrap();
    let rx = UdpSocket::bind(&addr).unwrap();

    let tx_addr = tx.local_addr().unwrap();
    let rx_addr = rx.local_addr().unwrap();
    assert!(tx.connect(rx_addr).is_ok());
    assert!(rx.connect(tx_addr).is_ok());

    test_send_recv_udp(tx, rx, true);
}

#[test]
pub fn test_udp_socket_discard() {
    let addr = localhost();
    let any = localhost();
    let outside = localhost();

    let tx = UdpSocket::bind(&any).unwrap();
    let rx = UdpSocket::bind(&addr).unwrap();
    let udp_outside = UdpSocket::bind(&outside).unwrap();

    let tx_addr = tx.local_addr().unwrap();
    let rx_addr = rx.local_addr().unwrap();
 
    assert!(tx.connect(rx_addr).is_ok());
    assert!(udp_outside.connect(rx_addr).is_ok());
    assert!(rx.connect(tx_addr).is_ok());

    let mut event_loop = EventLoop::new().unwrap();

    assert!(udp_outside.send("hello world".as_bytes()).is_ok());

    event_loop.register(&rx, LISTENER, Ready::readable(), PollOpt::edge()).unwrap();
    event_loop.register(&tx, SENDER, Ready::writable(), PollOpt::edge()).unwrap();

    event_loop.timeout(5000, time::Duration::from_secs(5)).unwrap();
    event_loop.run(&mut UdpHandlerTimeout {}).unwrap();
}