use std::io::ErrorKind;
use std::net::IpAddr;
use std::str;
use std::time;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use log::{debug, info};

use mio::net::UdpSocket;
use mio::{Events, Interests, Poll, Registry, Token};

mod util;

use util::{init, localhost};

const LISTENER: Token = Token(0);
const SENDER: Token = Token(1);

pub struct UdpHandlerSendRecv {
    tx: UdpSocket,
    rx: UdpSocket,
    msg: &'static str,
    buf: Bytes,
    rx_buf: BytesMut,
    connected: bool,
    shutdown: bool,
}

impl UdpHandlerSendRecv {
    fn new(tx: UdpSocket, rx: UdpSocket, connected: bool, msg: &'static str) -> UdpHandlerSendRecv {
        UdpHandlerSendRecv {
            tx,
            rx,
            msg,
            buf: Bytes::from_static(msg.as_bytes()),
            rx_buf: BytesMut::with_capacity(1024),
            connected,
            shutdown: false,
        }
    }
}

fn assert_send<T: Send>() {}

fn assert_sync<T: Sync>() {}

#[cfg(test)]
fn test_send_recv_udp(tx: UdpSocket, rx: UdpSocket, connected: bool) {
    debug!("Starting TEST_UDP_SOCKETS");
    let mut poll = Poll::new().unwrap();

    assert_send::<UdpSocket>();
    assert_sync::<UdpSocket>();

    // ensure that the sockets are non-blocking
    let mut buf = [0; 128];
    assert_eq!(
        ErrorKind::WouldBlock,
        rx.recv_from(&mut buf).unwrap_err().kind()
    );

    info!("Registering SENDER");
    poll.registry()
        .register(&tx, SENDER, Interests::WRITABLE)
        .unwrap();

    info!("Registering LISTENER");
    poll.registry()
        .register(&rx, LISTENER, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);

    info!("Starting event loop to test with...");
    let mut handler = UdpHandlerSendRecv::new(tx, rx, connected, "hello world");

    while !handler.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.is_readable() {
                if let LISTENER = event.token() {
                    debug!("We are receiving a datagram now...");
                    let cnt = unsafe {
                        if !handler.connected {
                            handler.rx.recv_from(handler.rx_buf.bytes_mut()).unwrap().0
                        } else {
                            handler.rx.recv(handler.rx_buf.bytes_mut()).unwrap()
                        }
                    };

                    unsafe {
                        BufMut::advance_mut(&mut handler.rx_buf, cnt);
                    }
                    assert!(str::from_utf8(handler.rx_buf.as_ref()).unwrap() == handler.msg);
                    handler.shutdown = true;
                }
            }

            if event.is_writable() {
                if let SENDER = event.token() {
                    let cnt = if !handler.connected {
                        let addr = handler.rx.local_addr().unwrap();
                        handler.tx.send_to(handler.buf.as_ref(), addr).unwrap()
                    } else {
                        handler.tx.send(handler.buf.as_ref()).unwrap()
                    };

                    handler.buf.advance(cnt);
                }
            }
        }
    }
}

/// Returns the sender and the receiver
fn connected_sockets() -> (UdpSocket, UdpSocket) {
    let addr = localhost();
    let any = localhost();

    let tx = UdpSocket::bind(any).unwrap();
    let rx = UdpSocket::bind(addr).unwrap();

    let tx_addr = tx.local_addr().unwrap();
    let rx_addr = rx.local_addr().unwrap();

    assert!(tx.connect(rx_addr).is_ok());
    assert!(rx.connect(tx_addr).is_ok());

    (tx, rx)
}

#[test]
pub fn test_udp_socket() {
    init();

    let addr = localhost();
    let any = localhost();

    let tx = UdpSocket::bind(any).unwrap();
    let rx = UdpSocket::bind(addr).unwrap();

    test_send_recv_udp(tx, rx, false);
}

#[test]
pub fn test_udp_socket_send_recv() {
    init();

    let (tx, rx) = connected_sockets();

    test_send_recv_udp(tx, rx, true);
}

#[test]
pub fn test_udp_socket_discard() {
    init();

    let addr = localhost();
    let any = localhost();
    let outside = localhost();

    let tx = UdpSocket::bind(any).unwrap();
    let rx = UdpSocket::bind(addr).unwrap();
    let udp_outside = UdpSocket::bind(outside).unwrap();

    let tx_addr = tx.local_addr().unwrap();
    let rx_addr = rx.local_addr().unwrap();

    assert!(tx.connect(rx_addr).is_ok());
    assert!(udp_outside.connect(rx_addr).is_ok());
    assert!(rx.connect(tx_addr).is_ok());

    let mut poll = Poll::new().unwrap();

    let r = udp_outside.send(b"hello world");
    assert!(r.is_ok() || r.unwrap_err().kind() == ErrorKind::WouldBlock);

    poll.registry()
        .register(&rx, LISTENER, Interests::READABLE)
        .unwrap();
    poll.registry()
        .register(&tx, SENDER, Interests::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);

    poll.poll(&mut events, Some(time::Duration::from_secs(5)))
        .unwrap();

    for event in &events {
        if event.is_readable() {
            if let LISTENER = event.token() {
                panic!("Expected to no receive a packet but got something")
            }
        }
    }
}

pub struct UdpHandler {
    tx: UdpSocket,
    rx: UdpSocket,
    msg: &'static str,
    buf: Bytes,
    rx_buf: BytesMut,
    localhost: IpAddr,
    shutdown: bool,
}

impl UdpHandler {
    fn new(tx: UdpSocket, rx: UdpSocket, msg: &'static str) -> UdpHandler {
        let sock = UdpSocket::bind("127.0.0.1:12345".parse().unwrap()).unwrap();
        UdpHandler {
            tx,
            rx,
            msg,
            buf: Bytes::from_static(msg.as_bytes()),
            rx_buf: BytesMut::with_capacity(1024),
            localhost: sock.local_addr().unwrap().ip(),
            shutdown: false,
        }
    }

    fn handle_read(&mut self, _: &Registry, token: Token) {
        if let LISTENER = token {
            debug!("We are receiving a datagram now...");
            match unsafe { self.rx.recv_from(self.rx_buf.bytes_mut()) } {
                Ok((cnt, addr)) => {
                    unsafe {
                        BufMut::advance_mut(&mut self.rx_buf, cnt);
                    }
                    assert_eq!(addr.ip(), self.localhost);
                }
                res => panic!("unexpected result: {:?}", res),
            }
            assert!(str::from_utf8(self.rx_buf.as_ref()).unwrap() == self.msg);
            self.shutdown = true;
        }
    }

    fn handle_write(&mut self, _: &Registry, token: Token) {
        if let SENDER = token {
            let addr = self.rx.local_addr().unwrap();
            let cnt = self.tx.send_to(self.buf.as_ref(), addr).unwrap();
            self.buf.advance(cnt);
        }
    }
}

// TODO: This doesn't pass on android 64bit CI...
// Figure out why!
#[cfg_attr(
    target_os = "android",
    ignore = "Multicast doesn't work on Android 64bit"
)]
#[test]
pub fn test_multicast() {
    init();

    debug!("Starting TEST_UDP_CONNECTIONLESS");
    let mut poll = Poll::new().unwrap();

    let addr = localhost();
    let any = "0.0.0.0:0".parse().unwrap();

    let tx = UdpSocket::bind(any).unwrap();
    let rx = UdpSocket::bind(addr).unwrap();

    info!("Joining group 227.1.1.100");
    let any = "0.0.0.0".parse().unwrap();
    rx.join_multicast_v4("227.1.1.100".parse().unwrap(), any)
        .unwrap();

    info!("Joining group 227.1.1.101");
    rx.join_multicast_v4("227.1.1.101".parse().unwrap(), any)
        .unwrap();

    info!("Registering SENDER");
    poll.registry()
        .register(&tx, SENDER, Interests::WRITABLE)
        .unwrap();

    info!("Registering LISTENER");
    poll.registry()
        .register(&rx, LISTENER, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);

    let mut handler = UdpHandler::new(tx, rx, "hello world");

    info!("Starting event loop to test with...");

    while !handler.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.is_readable() {
                handler.handle_read(poll.registry(), event.token());
            }

            if event.is_writable() {
                handler.handle_write(poll.registry(), event.token());
            }
        }
    }
}
