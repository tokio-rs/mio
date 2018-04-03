// TODO: This doesn't pass on android 64bit CI...
// Figure out why!
#![cfg(not(target_os = "android"))]

use mio::{Events, Poll, PollOpt, Ready, Token};
use mio::net::UdpSocket;
use bytes::BufMut;
use std::str;
use std::net::IpAddr;
use localhost;

const LISTENER: Token = Token(0);
const SENDER: Token = Token(1);

pub struct UdpHandler {
    tx: UdpSocket,
    rx: UdpSocket,
    msg: &'static str,
    buf: &'static [u8],
    rx_buf: Vec<u8>,
    localhost: IpAddr,
    shutdown: bool,
}

impl UdpHandler {
    fn new(tx: UdpSocket, rx: UdpSocket, msg: &'static str) -> UdpHandler {
        let sock = UdpSocket::bind(&"127.0.0.1:12345".parse().unwrap()).unwrap();
        UdpHandler {
            tx: tx,
            rx: rx,
            msg: msg,
            buf: msg.as_bytes(),
            rx_buf: Vec::with_capacity(1024),
            localhost: sock.local_addr().unwrap().ip(),
            shutdown: false,
        }
    }

    fn handle_read(&mut self, _: &mut Poll, token: Token, _: Ready) {
        match token {
            LISTENER => {
                debug!("We are receiving a datagram now...");
                match unsafe { self.rx.recv_from(self.rx_buf.bytes_mut()) } {
                    Ok((cnt, addr)) => {
                        unsafe { BufMut::advance_mut(&mut self.rx_buf, cnt); }
                        assert_eq!(addr.ip(), self.localhost);
                    }
                    res => panic!("unexpected result: {:?}", res),
                }
                assert!(str::from_utf8(self.rx_buf.as_ref()).unwrap() == self.msg);
                self.shutdown = true;
            },
            _ => ()
        }
    }

    fn handle_write(&mut self, _: &mut Poll, token: Token, _: Ready) {
        match token {
            SENDER => {
                let addr = self.rx.local_addr().unwrap();
                let cnt = self.tx.send_to(self.buf.as_ref(), &addr).unwrap();
                self.buf = &self.buf[cnt..];
            },
            _ => ()
        }
    }
}

#[test]
pub fn test_multicast() {
    drop(::env_logger::init());
    debug!("Starting TEST_UDP_CONNECTIONLESS");
    let mut poll = Poll::new().unwrap();

    let addr = localhost();
    let any = "0.0.0.0:0".parse().unwrap();

    let tx = UdpSocket::bind(&any).unwrap();
    let rx = UdpSocket::bind(&addr).unwrap();

    info!("Joining group 227.1.1.100");
    let any = "0.0.0.0".parse().unwrap();
    rx.join_multicast_v4(&"227.1.1.100".parse().unwrap(), &any).unwrap();

    info!("Joining group 227.1.1.101");
    rx.join_multicast_v4(&"227.1.1.101".parse().unwrap(), &any).unwrap();

    info!("Registering SENDER");
    poll..register().register(&tx, SENDER, Ready::WRITABLE, PollOpt::EDGE).unwrap();

    info!("Registering LISTENER");
    poll.register().register(&rx, LISTENER, Ready::READABLE, PollOpt::EDGE).unwrap();

    let mut events = Events::with_capacity(1024);

    let mut handler = UdpHandler::new(tx, rx, "hello world");

    info!("Starting event loop to test with...");

    while !handler.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.readiness().is_readable() {
                handler.handle_read(&mut poll, event.token(), event.readiness());
            }

            if event.readiness().is_writable() {
                handler.handle_write(&mut poll, event.token(), event.readiness());
            }
        }
    }
}
