use std::io::ErrorKind;
use std::net::{self, IpAddr, SocketAddr};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::str;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use log::{debug, info};

use mio::net::UdpSocket;
use mio::{Events, Interests, Poll, Registry, Token};

mod util;

use util::{
    any_local_address, any_local_ipv6_address, assert_error, assert_send, assert_sync,
    assert_would_block, expect_events, expect_no_events, init, init_with_poll, ExpectEvent,
};

const DATA1: &[u8] = b"Hello world!";
const DATA2: &[u8] = b"Hello mars!";

const LISTENER: Token = Token(0);
const SENDER: Token = Token(1);
const ID1: Token = Token(2);
const ID2: Token = Token(3);
const ID3: Token = Token(4);

#[test]
fn is_send_and_sync() {
    assert_send::<UdpSocket>();
    assert_sync::<UdpSocket>();
}

#[test]
fn unconnected_udp_socket_ipv4() {
    let socket1 = UdpSocket::bind(any_local_address()).unwrap();
    let socket2 = UdpSocket::bind(any_local_address()).unwrap();
    smoke_test_unconnected_udp_socket(socket1, socket2);
}

#[test]
fn unconnected_udp_socket_ipv6() {
    let socket1 = UdpSocket::bind(any_local_ipv6_address()).unwrap();
    let socket2 = UdpSocket::bind(any_local_ipv6_address()).unwrap();
    smoke_test_unconnected_udp_socket(socket1, socket2);
}

fn smoke_test_unconnected_udp_socket(socket1: UdpSocket, socket2: UdpSocket) {
    let (mut poll, mut events) = init_with_poll();

    let address1 = socket1.local_addr().unwrap();
    let address2 = socket2.local_addr().unwrap();

    poll.registry()
        .register(&socket1, ID1, Interests::READABLE.add(Interests::WRITABLE))
        .expect("unable to register UDP socket");
    poll.registry()
        .register(&socket2, ID2, Interests::READABLE.add(Interests::WRITABLE))
        .expect("unable to register UDP socket");

    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID1, Interests::WRITABLE),
            ExpectEvent::new(ID2, Interests::WRITABLE),
        ],
    );

    let mut buf = [0; 20];
    assert_would_block(socket1.peek_from(&mut buf));
    assert_would_block(socket1.recv_from(&mut buf));

    socket1.send_to(DATA1, address2).unwrap();
    socket2.send_to(DATA2, address1).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID1, Interests::READABLE),
            ExpectEvent::new(ID2, Interests::READABLE),
        ],
    );

    let (n, got_address1) = socket1.peek_from(&mut buf).unwrap();
    assert_eq!(n, DATA2.len());
    assert_eq!(buf[..n], DATA2[..]);
    assert_eq!(got_address1, address2);

    let (n, got_address2) = socket2.peek_from(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);
    assert_eq!(got_address2, address1);

    let (n, got_address1) = socket1.recv_from(&mut buf).unwrap();
    assert_eq!(n, DATA2.len());
    assert_eq!(buf[..n], DATA2[..]);
    assert_eq!(got_address1, address2);

    let (n, got_address2) = socket2.recv_from(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);
    assert_eq!(got_address2, address1);

    assert!(socket1.take_error().unwrap().is_none());
    assert!(socket2.take_error().unwrap().is_none());
}

#[test]
fn connected_udp_socket_ipv4() {
    let socket1 = UdpSocket::bind(any_local_address()).unwrap();
    let address1 = socket1.local_addr().unwrap();

    let socket2 = UdpSocket::bind(any_local_address()).unwrap();
    let address2 = socket2.local_addr().unwrap();

    socket1.connect(address2).unwrap();
    socket2.connect(address1).unwrap();

    smoke_test_connected_udp_socket(socket1, socket2);
}

#[test]
fn connected_udp_socket_ipv6() {
    let socket1 = UdpSocket::bind(any_local_ipv6_address()).unwrap();
    let address1 = socket1.local_addr().unwrap();

    let socket2 = UdpSocket::bind(any_local_ipv6_address()).unwrap();
    let address2 = socket2.local_addr().unwrap();

    socket1.connect(address2).unwrap();
    socket2.connect(address1).unwrap();

    smoke_test_connected_udp_socket(socket1, socket2);
}

fn smoke_test_connected_udp_socket(socket1: UdpSocket, socket2: UdpSocket) {
    let (mut poll, mut events) = init_with_poll();

    poll.registry()
        .register(&socket1, ID1, Interests::READABLE.add(Interests::WRITABLE))
        .expect("unable to register UDP socket");
    poll.registry()
        .register(&socket2, ID2, Interests::READABLE.add(Interests::WRITABLE))
        .expect("unable to register UDP socket");

    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID1, Interests::WRITABLE),
            ExpectEvent::new(ID2, Interests::WRITABLE),
        ],
    );

    let mut buf = [0; 20];
    assert_would_block(socket1.peek(&mut buf));
    assert_would_block(socket1.recv(&mut buf));

    socket1.send(DATA1).unwrap();
    socket2.send(DATA2).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID1, Interests::READABLE),
            ExpectEvent::new(ID2, Interests::READABLE),
        ],
    );

    let mut buf = [0; 20];
    let n = socket1.peek(&mut buf).unwrap();
    assert_eq!(n, DATA2.len());
    assert_eq!(buf[..n], DATA2[..]);

    let n = socket2.peek(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);

    let n = socket1.recv(&mut buf).unwrap();
    assert_eq!(n, DATA2.len());
    assert_eq!(buf[..n], DATA2[..]);

    let n = socket2.recv(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);

    assert!(socket1.take_error().unwrap().is_none());
    assert!(socket2.take_error().unwrap().is_none());
}

#[test]
fn reconnect_udp_socket_sending() {
    let (mut poll, mut events) = init_with_poll();

    let socket1 = UdpSocket::bind(any_local_address()).unwrap();
    let socket2 = UdpSocket::bind(any_local_address()).unwrap();
    let socket3 = UdpSocket::bind(any_local_address()).unwrap();

    let address1 = socket1.local_addr().unwrap();
    let address2 = socket2.local_addr().unwrap();
    let address3 = socket3.local_addr().unwrap();

    socket1.connect(address2).unwrap();
    socket2.connect(address1).unwrap();
    socket3.connect(address1).unwrap();

    poll.registry()
        .register(&socket1, ID1, Interests::READABLE.add(Interests::WRITABLE))
        .unwrap();
    poll.registry()
        .register(&socket2, ID2, Interests::READABLE)
        .unwrap();
    poll.registry()
        .register(&socket3, ID3, Interests::READABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::WRITABLE)],
    );

    socket1.send(DATA1).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID2, Interests::READABLE)],
    );

    let mut buf = [0; 20];
    let n = socket2.recv(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);

    socket1.connect(address3).unwrap();
    socket1.send(DATA2).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID3, Interests::READABLE)],
    );

    let n = socket3.recv(&mut buf).unwrap();
    assert_eq!(n, DATA2.len());
    assert_eq!(buf[..n], DATA2[..]);

    assert!(socket1.take_error().unwrap().is_none());
    assert!(socket2.take_error().unwrap().is_none());
    assert!(socket3.take_error().unwrap().is_none());
}

#[test]
#[cfg_attr(windows, ignore = "fails on Windows, see #1080")]
fn reconnect_udp_socket_receiving() {
    let (mut poll, mut events) = init_with_poll();

    let socket1 = UdpSocket::bind(any_local_address()).unwrap();
    let socket2 = UdpSocket::bind(any_local_address()).unwrap();
    let socket3 = UdpSocket::bind(any_local_address()).unwrap();

    let address1 = socket1.local_addr().unwrap();
    let address2 = socket2.local_addr().unwrap();
    let address3 = socket3.local_addr().unwrap();

    socket1.connect(address2).unwrap();
    socket2.connect(address1).unwrap();
    socket3.connect(address1).unwrap();

    poll.registry()
        .register(&socket1, ID1, Interests::READABLE)
        .unwrap();
    poll.registry()
        .register(&socket2, ID2, Interests::WRITABLE)
        .unwrap();
    poll.registry()
        .register(&socket3, ID3, Interests::WRITABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID2, Interests::WRITABLE),
            ExpectEvent::new(ID3, Interests::WRITABLE),
        ],
    );

    socket2.send(DATA1).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );

    let mut buf = [0; 20];
    let n = socket1.recv(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);

    socket1.connect(address3).unwrap();
    socket3.send(DATA2).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );

    // Read only a part of the data.
    let max = 4;
    let n = socket1.recv(&mut buf[..max]).unwrap();
    assert_eq!(n, max);
    assert_eq!(buf[..max], DATA2[..max]);

    // Now connect back to socket 2, dropping the unread data.
    socket1.connect(address2).unwrap();
    socket2.send(DATA2).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );

    let n = socket1.recv(&mut buf).unwrap();
    assert_eq!(n, DATA2.len());
    assert_eq!(buf[..n], DATA2[..]);

    assert!(socket1.take_error().unwrap().is_none());
    assert!(socket2.take_error().unwrap().is_none());
    assert!(socket3.take_error().unwrap().is_none());
}

#[test]
#[cfg_attr(windows, ignore = "fails on Windows, see #1080")]
fn unconnected_udp_socket_connected_methods() {
    let (mut poll, mut events) = init_with_poll();

    let socket1 = UdpSocket::bind(any_local_address()).unwrap();
    let socket2 = UdpSocket::bind(any_local_address()).unwrap();
    let address2 = socket2.local_addr().unwrap();

    poll.registry()
        .register(&socket1, ID1, Interests::WRITABLE)
        .unwrap();
    poll.registry()
        .register(&socket2, ID2, Interests::READABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::WRITABLE)],
    );

    // Socket is unconnected, but we're using an connected method.
    assert_error(socket1.send(DATA1), "address required");

    // Now send some actual data.
    socket1.send_to(DATA1, address2).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID2, Interests::READABLE)],
    );

    // Receive methods don't require the socket to be connected, you just won't
    // know the sender.
    let mut buf = [0; 20];
    let n = socket2.peek(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);

    let n = socket2.recv(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);

    assert!(socket1.take_error().unwrap().is_none());
    assert!(socket2.take_error().unwrap().is_none());
}

#[test]
fn connected_udp_socket_unconnected_methods() {
    let (mut poll, mut events) = init_with_poll();

    let socket1 = UdpSocket::bind(any_local_address()).unwrap();
    let socket2 = UdpSocket::bind(any_local_address()).unwrap();
    let socket3 = UdpSocket::bind(any_local_address()).unwrap();

    let address2 = socket2.local_addr().unwrap();
    let address3 = socket3.local_addr().unwrap();

    socket1.connect(address3).unwrap();
    socket3.connect(address2).unwrap();

    poll.registry()
        .register(&socket1, ID1, Interests::WRITABLE)
        .unwrap();
    poll.registry()
        .register(&socket2, ID2, Interests::WRITABLE)
        .unwrap();
    poll.registry()
        .register(&socket3, ID3, Interests::READABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![
            ExpectEvent::new(ID1, Interests::WRITABLE),
            ExpectEvent::new(ID2, Interests::WRITABLE),
        ],
    );

    // Can't use `send_to`.
    // Linux (and Android) and Windows actually allow `send_to` even if the
    // socket is connected.
    #[cfg(not(any(target_os = "android", target_os = "linux", target_os = "windows")))]
    assert_error(socket1.send_to(DATA1, address2), "already connected");
    // Even if the address is the same.
    #[cfg(not(any(target_os = "android", target_os = "linux", target_os = "windows")))]
    assert_error(socket1.send_to(DATA1, address3), "already connected");

    socket2.send_to(DATA2, address3).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID3, Interests::READABLE)],
    );

    let mut buf = [0; 20];
    let (n, got_address1) = socket3.peek_from(&mut buf).unwrap();
    assert_eq!(n, DATA2.len());
    assert_eq!(buf[..n], DATA2[..]);
    assert_eq!(got_address1, address2);

    let (n, got_address2) = socket3.recv_from(&mut buf).unwrap();
    assert_eq!(n, DATA2.len());
    assert_eq!(buf[..n], DATA2[..]);
    assert_eq!(got_address2, address2);

    assert!(socket1.take_error().unwrap().is_none());
    assert!(socket2.take_error().unwrap().is_none());
    assert!(socket3.take_error().unwrap().is_none());
}

#[test]
#[cfg(unix)]
fn udp_socket_raw_fd() {
    init();

    let socket = UdpSocket::bind(any_local_address()).unwrap();
    let address = socket.local_addr().unwrap();

    let raw_fd1 = socket.as_raw_fd();
    let raw_fd2 = socket.into_raw_fd();
    assert_eq!(raw_fd1, raw_fd2);

    let socket = unsafe { UdpSocket::from_raw_fd(raw_fd2) };
    assert_eq!(socket.as_raw_fd(), raw_fd1);
    assert_eq!(socket.local_addr().unwrap(), address);
}

#[test]
fn udp_socket_register() {
    let (mut poll, mut events) = init_with_poll();

    let socket = UdpSocket::bind(any_local_address()).unwrap();
    poll.registry()
        .register(&socket, ID1, Interests::READABLE)
        .expect("unable to register UDP socket");

    expect_no_events(&mut poll, &mut events);

    // NOTE: more tests are done in the smoke tests above.
}

#[test]
fn udp_socket_reregister() {
    let (mut poll, mut events) = init_with_poll();

    let socket = UdpSocket::bind(any_local_address()).unwrap();
    let address = socket.local_addr().unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let thread_handle = send_packets(address, 1, barrier.clone());

    poll.registry()
        .register(&socket, ID1, Interests::WRITABLE)
        .unwrap();
    // Let the first packet be send.
    barrier.wait();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::WRITABLE)], // Not readable!
    );

    poll.registry()
        .reregister(&socket, ID2, Interests::READABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID2, Interests::READABLE)],
    );

    let mut buf = [0; 20];
    let (n, _) = socket.recv_from(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);

    thread_handle.join().expect("unable to join thread");
}

#[test]
#[cfg_attr(windows, ignore = "fails on Windows, see #1080")]
fn udp_socket_deregister() {
    let (mut poll, mut events) = init_with_poll();

    let socket = UdpSocket::bind(any_local_address()).unwrap();
    let address = socket.local_addr().unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let thread_handle = send_packets(address, 1, barrier.clone());

    poll.registry()
        .register(&socket, ID1, Interests::READABLE)
        .unwrap();

    // Let the packet be send.
    barrier.wait();

    poll.registry().deregister(&socket).unwrap();

    expect_no_events(&mut poll, &mut events);

    // But we do expect a packet to be send.
    let mut buf = [0; 20];
    let (n, _) = socket.recv_from(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(buf[..n], DATA1[..]);

    thread_handle.join().expect("unable to join thread");
}

/// Sends `n_packets` packets to `address`, over UDP, after the `barrier` is
/// waited (before each send) on in another thread.
fn send_packets(
    address: SocketAddr,
    n_packets: usize,
    barrier: Arc<Barrier>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let socket = net::UdpSocket::bind(any_local_address()).unwrap();
        for _ in 0..n_packets {
            barrier.wait();
            assert_eq!(socket.send_to(DATA1, address).unwrap(), DATA1.len());
        }
    })
}

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

#[cfg(test)]
fn send_recv_udp(tx: UdpSocket, rx: UdpSocket, connected: bool) {
    init();

    debug!("Starting TEST_UDP_SOCKETS");
    let mut poll = Poll::new().unwrap();

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
    let tx = UdpSocket::bind(any_local_address()).unwrap();
    let rx = UdpSocket::bind(any_local_address()).unwrap();

    let tx_addr = tx.local_addr().unwrap();
    let rx_addr = rx.local_addr().unwrap();

    assert!(tx.connect(rx_addr).is_ok());
    assert!(rx.connect(tx_addr).is_ok());

    (tx, rx)
}

#[test]
pub fn udp_socket() {
    init();

    let tx = UdpSocket::bind(any_local_address()).unwrap();
    let rx = UdpSocket::bind(any_local_address()).unwrap();

    send_recv_udp(tx, rx, false);
}

#[test]
pub fn udp_socket_send_recv() {
    init();

    let (tx, rx) = connected_sockets();

    send_recv_udp(tx, rx, true);
}

#[test]
pub fn udp_socket_discard() {
    init();

    let tx = UdpSocket::bind(any_local_address()).unwrap();
    let rx = UdpSocket::bind(any_local_address()).unwrap();
    let udp_outside = UdpSocket::bind(any_local_address()).unwrap();

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

    poll.poll(&mut events, Some(Duration::from_secs(5)))
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
pub fn multicast() {
    init();

    debug!("Starting TEST_UDP_CONNECTIONLESS");
    let mut poll = Poll::new().unwrap();

    let tx = UdpSocket::bind(any_local_address()).unwrap();
    let rx = UdpSocket::bind(any_local_address()).unwrap();

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
