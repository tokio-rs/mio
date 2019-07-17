use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::Shutdown;
use std::ops::DerefMut;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use std::{cmp, net, thread};

use log::debug;
use net2::{self, TcpStreamExt};

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interests, Poll, Token};

use crate::util::{assert_send, assert_sync, localhost, TryRead, TryWrite};

const LISTENER: Token = Token(0);
const STREAM: Token = Token(1);
const STREAM2: Token = Token(2);

const N: usize = 16 * 1024 * 1024;

#[test]
fn is_send_and_sync() {
    assert_send::<TcpStream>();
    assert_sync::<TcpStream>();
}

#[test]
fn connecting() {
    struct H {
        hit: u32,
        shutdown: bool,
    }

    let l = net::TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();

    let (tx, rx) = channel();
    let (tx2, rx2) = channel();
    let t = thread::spawn(move || {
        let s = l.accept().unwrap();
        rx.recv().unwrap();
        drop(s);
        tx2.send(()).unwrap();
    });

    let mut poll = Poll::new().unwrap();
    let s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, STREAM, Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        hit: 0,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), STREAM);
            match h.hit {
                0 => assert!(event.is_writable()),
                1 => assert!(event.is_readable()),
                _ => panic!(),
            }
            h.hit += 1;
            h.shutdown = true;
        }
    }
    assert_eq!(h.hit, 1);
    tx.send(()).unwrap();
    rx2.recv().unwrap();
    h.shutdown = false;
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), STREAM);
            match h.hit {
                0 => assert!(event.is_writable()),
                1 => assert!(event.is_readable()),
                _ => panic!(),
            }
            h.hit += 1;
            h.shutdown = true;
        }
    }
    assert_eq!(h.hit, 2);
    t.join().unwrap();
}

#[test]
fn connect_then_close() {
    struct H {
        listener: TcpListener,
        shutdown: bool,
    }

    let mut poll = Poll::new().unwrap();
    let l = TcpListener::bind(localhost()).unwrap();
    let s = TcpStream::connect(l.local_addr().unwrap()).unwrap();

    poll.registry()
        .register(&l, LISTENER, Interests::READABLE)
        .unwrap();
    poll.registry()
        .register(&s, STREAM, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        listener: l,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == LISTENER {
                let s = h.listener.accept().unwrap().0;
                poll.registry()
                    .register(&s, STREAM2, Interests::READABLE | Interests::WRITABLE)
                    .unwrap();
                drop(s);
            } else if event.token() == STREAM {
                h.shutdown = true;
            }
        }
    }
}

#[test]
fn connecting_error() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);

    // Pick a "random" port that shouldn't be in use.
    let s = match TcpStream::connect("127.0.0.1:38381".parse().unwrap()) {
        Ok(l) => l,
        Err(ref e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            // Connection failed synchronously. This is not a bug, but it
            // unfortunately doesn't get us the code coverage we want.
            return;
        }
        Err(e) => panic!("TcpStream::connect unexpected error {:?}", e),
    };

    poll.registry()
        .register(&s, STREAM, Interests::WRITABLE)
        .unwrap();

    'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == STREAM {
                assert!(event.is_writable());
                break 'outer;
            }
        }
    }

    assert!(s.take_error().unwrap().is_some());
}

#[test]
fn reading() {
    struct H {
        amt: usize,
        socket: TcpStream,
        shutdown: bool,
    }

    let l = net::TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let b = [0; 1024];
        let mut amt = 0;
        while amt < N {
            amt += s.write(&b).unwrap();
        }
    });

    let mut poll = Poll::new().unwrap();
    let s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, STREAM, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        amt: 0,
        socket: s,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), STREAM);
            let mut b = [0; 1024];
            loop {
                if let Some(amt) = h.socket.try_read(&mut b).unwrap() {
                    h.amt += amt;
                } else {
                    break;
                }
                if h.amt >= N {
                    h.shutdown = true;
                    break;
                }
            }
        }
    }
    t.join().unwrap();
}

#[test]
fn peeking() {
    struct H {
        amt: usize,
        socket: TcpStream,
        shutdown: bool,
    }

    let l = net::TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let b = [0; 1024];
        let mut amt = 0;
        while amt < N {
            amt += s.write(&b).unwrap();
        }
    });

    let mut poll = Poll::new().unwrap();
    let s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, STREAM, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        amt: 0,
        socket: s,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), STREAM);
            let mut b = [0; 1024];
            match h.socket.peek(&mut b) {
                Ok(_) => (),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => panic!("unexpected error: {:?}", e),
            }

            loop {
                if let Some(amt) = h.socket.try_read(&mut b).unwrap() {
                    h.amt += amt;
                } else {
                    break;
                }
                if h.amt >= N {
                    h.shutdown = true;
                    break;
                }
            }
        }
    }
    t.join().unwrap();
}

#[test]
fn read_vectored() {
    let l = net::TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let b = [1; 1024];
        let mut amt = 0;
        while amt < N {
            amt += s.write(&b).unwrap();
        }
    });

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(128);

    let s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, STREAM, Interests::READABLE)
        .unwrap();

    let b1 = &mut [0; 10][..];
    let b2 = &mut [0; 383][..];
    let b3 = &mut [0; 28][..];
    let b4 = &mut [0; 8][..];
    let b5 = &mut [0; 128][..];
    let mut b: [IoSliceMut; 5] = [
        IoSliceMut::new(b1),
        IoSliceMut::new(b2),
        IoSliceMut::new(b3),
        IoSliceMut::new(b4),
        IoSliceMut::new(b5),
    ];

    let mut so_far = 0;
    'event_loop: loop {
        for buf in b.iter_mut() {
            for byte in buf.deref_mut().iter_mut() {
                *byte = 0;
            }
        }

        poll.poll(&mut events, None).unwrap();

        'read_loop: loop {
            match (&s).read_vectored(&mut b) {
                Ok(0) => {
                    assert_eq!(so_far, N);
                    break 'event_loop;
                }
                Ok(mut n) => {
                    so_far += n;
                    for buf in b.iter() {
                        for byte in (&buf[..cmp::min(n, buf.len())]).iter() {
                            assert_eq!(*byte, 1);
                        }
                        n = n.saturating_sub(buf.len());
                        if n == 0 {
                            continue 'read_loop;
                        }
                    }
                    assert_eq!(n, 0);
                }
                Err(e) => {
                    assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
                    break 'read_loop;
                }
            }
        }
    }

    t.join().unwrap();
}

#[test]
fn writing() {
    struct H {
        amt: usize,
        socket: TcpStream,
        shutdown: bool,
    }

    let l = net::TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let mut b = [0; 1024];
        let mut amt = 0;
        while amt < N {
            amt += s.read(&mut b).unwrap();
        }
    });

    let mut poll = Poll::new().unwrap();
    let s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&s, STREAM, Interests::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        amt: 0,
        socket: s,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), STREAM);
            let b = [0; 1024];
            loop {
                if let Some(amt) = h.socket.try_write(&b).unwrap() {
                    h.amt += amt;
                } else {
                    break;
                }
                if h.amt >= N {
                    h.shutdown = true;
                    break;
                }
            }
        }
    }
    t.join().unwrap();
}

#[test]
fn write_vectored() {
    let l = net::TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let mut b = [0; 1024];
        let mut amt = 0;
        while amt < N {
            for byte in b.iter_mut() {
                *byte = 0;
            }
            let n = s.read(&mut b).unwrap();
            amt += n;
            for byte in b[..n].iter() {
                assert_eq!(*byte, 1);
            }
        }
    });

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(128);
    let s = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&s, STREAM, Interests::WRITABLE)
        .unwrap();

    let b1 = &mut [1; 10][..];
    let b2 = &mut [1; 383][..];
    let b3 = &mut [1; 28][..];
    let b4 = &mut [1; 8][..];
    let b5 = &mut [1; 128][..];
    let b: [IoSlice; 5] = [
        IoSlice::new(b1),
        IoSlice::new(b2),
        IoSlice::new(b3),
        IoSlice::new(b4),
        IoSlice::new(b5),
    ];

    let mut so_far = 0;
    while so_far < N {
        poll.poll(&mut events, None).unwrap();

        loop {
            match (&s).write_vectored(&b) {
                Ok(n) => so_far += n,
                Err(e) => {
                    // FIXME: get Other error on macOS in release mode.
                    assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
                    break;
                }
            }
        }
    }

    t.join().unwrap();
}

#[test]
fn write_error() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);
    let (tx, rx) = channel();

    let listener = net::TcpListener::bind(localhost()).unwrap();
    let addr = listener.local_addr().unwrap();
    let t = thread::spawn(move || {
        let (conn, _addr) = listener.accept().unwrap();
        rx.recv().unwrap();
        drop(conn);
    });

    let mut s = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&s, STREAM, Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    let mut wait_writable = || 'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == STREAM && event.is_writable() {
                break 'outer;
            }
        }
    };

    wait_writable();

    tx.send(()).unwrap();
    t.join().unwrap();

    let buf = [0; 1024];
    loop {
        match s.write(&buf) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => wait_writable(),
            Err(e) => {
                debug!("good error: {}", e);
                break;
            }
        }
    }
}

#[test]
fn multiple_writes_immediate_success() {
    const N: usize = 16;
    let l = net::TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        let mut s = l.accept().unwrap().0;
        let mut b = [0; 1024];
        let mut amt = 0;
        while amt < 1024 * N {
            for byte in b.iter_mut() {
                *byte = 0;
            }
            let n = s.read(&mut b).unwrap();
            amt += n;
            for byte in b[..n].iter() {
                assert_eq!(*byte, 1);
            }
        }
    });

    let mut poll = Poll::new().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&s, STREAM, Interests::WRITABLE)
        .unwrap();
    let mut events = Events::with_capacity(16);

    // Wait for our TCP stream to connect
    'outer: loop {
        poll.poll(&mut events, None).unwrap();
        for event in events.iter() {
            if event.token() == STREAM && event.is_writable() {
                break 'outer;
            }
        }
    }

    for _ in 0..N {
        s.write_all(&[1; 1024]).unwrap();
    }

    t.join().unwrap();
}

#[test]
fn write_then_drop() {
    drop(env_logger::try_init());

    let l = TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();

    let mut poll = Poll::new().unwrap();
    let registry = poll.registry().clone();

    registry
        .register(&l, LISTENER, Interests::READABLE)
        .unwrap();

    registry.register(&s, STREAM, Interests::READABLE).unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), LISTENER);

    let mut s2 = l.accept().unwrap().0;

    registry
        .register(&s2, STREAM2, Interests::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), STREAM2);

    s2.write_all(&[1, 2, 3, 4]).unwrap();
    drop(s2);

    registry
        .reregister(&s, STREAM, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), STREAM);

    let mut buf = [0; 10];
    assert_eq!(s.read(&mut buf).unwrap(), 4);
    assert_eq!(&buf[0..4], &[1, 2, 3, 4]);
}

#[test]
fn write_then_deregister() {
    drop(env_logger::try_init());

    let l = TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();

    let mut poll = Poll::new().unwrap();
    let registry = poll.registry().clone();

    registry
        .register(&l, LISTENER, Interests::READABLE)
        .unwrap();
    registry.register(&s, STREAM, Interests::READABLE).unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), LISTENER);

    let mut s2 = l.accept().unwrap().0;

    registry
        .register(&s2, STREAM2, Interests::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), STREAM2);

    s2.write_all(&[1, 2, 3, 4]).unwrap();
    registry.deregister(&s2).unwrap();

    registry
        .reregister(&s, STREAM, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), STREAM);

    let mut buf = [0; 10];
    assert_eq!(s.read(&mut buf).unwrap(), 4);
    assert_eq!(&buf[0..4], &[1, 2, 3, 4]);
}

#[test]
fn connection_reset_by_peer() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);
    let mut buf = [0u8; 16];

    // Create listener
    let l = TcpListener::bind(localhost()).unwrap();
    let addr = l.local_addr().unwrap();

    // Connect client
    let client = net2::TcpBuilder::new_v4().unwrap().to_tcp_stream().unwrap();

    client.set_linger(Some(Duration::from_millis(0))).unwrap();
    client.connect(&addr).unwrap();

    // Convert to Mio stream
    let client = TcpStream::from_stream(client).unwrap();

    // Register server
    poll.registry()
        .register(&l, LISTENER, Interests::READABLE)
        .unwrap();

    // Register interest in the client
    poll.registry()
        .register(&client, STREAM, Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    // Wait for listener to be ready
    let mut accept_stream;
    'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == LISTENER {
                match l.accept() {
                    Ok((sock, _)) => {
                        accept_stream = sock;
                        break 'outer;
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => panic!("unexpected error {:?}", e),
                }
            }
        }
    }

    // Close the connection
    drop(client);

    // Wait a moment
    thread::sleep(Duration::from_millis(100));

    poll.registry()
        .register(&accept_stream, STREAM, Interests::READABLE)
        .unwrap();

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == STREAM {
                assert!(event.is_readable());

                match accept_stream.read(&mut buf) {
                    Ok(0) | Err(_) => {}

                    Ok(x) => panic!("expected empty buffer but read {} bytes", x),
                }
                return;
            }
        }
    }
}

macro_rules! wait {
    ($poll:ident, $ready:ident, $expect_hup: expr) => {{
        let now = Instant::now();
        let mut events = Events::with_capacity(16);
        let mut found = false;

        while !found {
            if now.elapsed() > Duration::from_secs(5) {
                panic!("not ready");
            }

            $poll
                .poll(&mut events, Some(Duration::from_secs(1)))
                .unwrap();

            for event in &events {
                // Hup is only generated on kqueue platforms.
                #[cfg(any(
                    target_os = "bitrig",
                    target_os = "dragonfly",
                    target_os = "freebsd",
                    target_os = "ios",
                    target_os = "macos",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                {
                    if $expect_hup {
                        assert!(event.is_hup());
                    }
                }

                if !$expect_hup {
                    assert!(!event.is_hup());
                }

                if event.token() == STREAM && event.$ready() {
                    found = true;
                    break;
                }
            }
        }
    }};
}

#[test]
fn test_write_shutdown() {
    let mut poll = Poll::new().unwrap();

    let listener = net::TcpListener::bind(localhost()).unwrap();
    let addr = listener.local_addr().unwrap();

    let client = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&client, STREAM, Interests::READABLE | Interests::WRITABLE)
        .unwrap();

    let (socket, _) = listener.accept().unwrap();

    wait!(poll, is_writable, false);

    let mut events = Events::with_capacity(16);

    // Polling should not have any events
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();
    assert!(events.is_empty());

    debug!("shutting down");
    // Now, shutdown the write half of the socket.
    socket.shutdown(Shutdown::Write).unwrap();

    wait!(poll, is_readable, true);
}

struct MyHandler {
    listener: TcpListener,
    connected: TcpStream,
    accepted: Option<TcpStream>,
    shutdown: bool,
}

#[test]
fn local_addr_ready() {
    let server = TcpListener::bind(localhost()).unwrap();
    let addr = server.local_addr().unwrap();

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&server, LISTENER, Interests::READABLE)
        .unwrap();

    let sock = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&sock, STREAM, Interests::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);

    let mut handler = MyHandler {
        listener: server,
        connected: sock,
        accepted: None,
        shutdown: false,
    };

    while !handler.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            match event.token() {
                LISTENER => {
                    let sock = handler.listener.accept().unwrap().0;
                    poll.registry()
                        .register(&sock, STREAM2, Interests::WRITABLE)
                        .unwrap();
                    handler.accepted = Some(sock);
                }
                STREAM2 => {
                    handler.accepted.as_ref().unwrap().peer_addr().unwrap();
                    handler.accepted.as_ref().unwrap().local_addr().unwrap();
                    handler
                        .accepted
                        .as_mut()
                        .unwrap()
                        .try_write(&[1, 2, 3])
                        .unwrap();
                    handler.accepted = None;
                }
                STREAM => {
                    handler.connected.peer_addr().unwrap();
                    handler.connected.local_addr().unwrap();
                    handler.shutdown = true;
                }
                _ => panic!("unexpected token"),
            }
        }
    }
}
