#![cfg(all(feature = "os-poll", feature = "tcp"))]

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
#[cfg(unix)]
use net2::TcpStreamExt;
use std::io::{self, Read, Write};
use std::net::{self, Shutdown};
#[cfg(unix)]
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::sync::mpsc::channel;
use std::thread::{self, sleep};
use std::time::Duration;

#[macro_use]
mod util;
use util::{
    any_local_address, assert_send, assert_sync, expect_events, expect_no_events, init,
    init_with_poll, ExpectEvent,
};

const LISTEN: Token = Token(0);
const CLIENT: Token = Token(1);
const SERVER: Token = Token(2);

#[test]
fn is_send_and_sync() {
    assert_send::<TcpListener>();
    assert_sync::<TcpListener>();

    assert_send::<TcpStream>();
    assert_sync::<TcpStream>();
}

#[test]
fn accept() {
    init();

    struct H {
        hit: bool,
        listener: TcpListener,
        shutdown: bool,
    }

    let mut l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = l.local_addr().unwrap();

    let t = thread::spawn(move || {
        net::TcpStream::connect(addr).unwrap();
    });

    let mut poll = Poll::new().unwrap();

    poll.registry()
        .register(&mut l, Token(1), Interest::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        hit: false,
        listener: l,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            h.hit = true;
            assert_eq!(event.token(), Token(1));
            assert!(event.is_readable());
            assert!(h.listener.accept().is_ok());
            h.shutdown = true;
        }
    }
    assert!(h.hit);
    assert!(h.listener.accept().unwrap_err().kind() == io::ErrorKind::WouldBlock);
    t.join().unwrap();
}

#[test]
fn connect() {
    init();

    struct H {
        hit: u32,
        shutdown: bool,
    }

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
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
    let mut s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&mut s, Token(1), Interest::READABLE | Interest::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        hit: 0,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            assert_eq!(event.token(), Token(1));
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
            assert_eq!(event.token(), Token(1));
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
fn read() {
    init();

    const N: usize = 16 * 1024 * 1024;
    struct H {
        amt: usize,
        socket: TcpStream,
        shutdown: bool,
    }

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
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
    let mut s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&mut s, Token(1), Interest::READABLE)
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
            assert_eq!(event.token(), Token(1));
            let mut b = [0; 1024];
            loop {
                if let Ok(amt) = h.socket.read(&mut b) {
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
fn peek() {
    init();

    const N: usize = 16 * 1024 * 1024;
    struct H {
        amt: usize,
        socket: TcpStream,
        shutdown: bool,
    }

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
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
    let mut s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&mut s, Token(1), Interest::READABLE)
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
            assert_eq!(event.token(), Token(1));
            let mut b = [0; 1024];
            match h.socket.peek(&mut b) {
                Ok(_) => (),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => panic!("unexpected error: {:?}", e),
            }

            loop {
                if let Ok(amt) = h.socket.read(&mut b) {
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
fn write() {
    init();

    const N: usize = 16 * 1024 * 1024;
    struct H {
        amt: usize,
        socket: TcpStream,
        shutdown: bool,
    }

    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
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
    let mut s = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&mut s, Token(1), Interest::WRITABLE)
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
            assert_eq!(event.token(), Token(1));
            let b = [0; 1024];
            loop {
                if let Ok(amt) = h.socket.write(&b) {
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
fn connect_then_close() {
    init();

    struct H {
        listener: TcpListener,
        shutdown: bool,
    }

    let mut poll = Poll::new().unwrap();
    let mut l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let mut s = TcpStream::connect(l.local_addr().unwrap()).unwrap();

    poll.registry()
        .register(&mut l, Token(1), Interest::READABLE)
        .unwrap();
    poll.registry()
        .register(&mut s, Token(2), Interest::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(128);

    let mut h = H {
        listener: l,
        shutdown: false,
    };
    while !h.shutdown {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(1) {
                let mut s = h.listener.accept().unwrap().0;
                poll.registry()
                    .register(&mut s, Token(3), Interest::READABLE | Interest::WRITABLE)
                    .unwrap();
                drop(s);
            } else if event.token() == Token(2) {
                h.shutdown = true;
            }
        }
    }
}

#[test]
fn listen_then_close() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();

    poll.registry()
        .register(&mut l, Token(1), Interest::READABLE)
        .unwrap();
    drop(l);

    let mut events = Events::with_capacity(128);

    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();

    for event in &events {
        if event.token() == Token(1) {
            panic!("recieved ready() on a closed TcpListener")
        }
    }
}

#[test]
fn bind_twice_bad() {
    init();

    let l1 = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = l1.local_addr().unwrap();
    assert!(TcpListener::bind(addr).is_err());
}

#[test]
fn multiple_writes_immediate_success() {
    init();

    const N: usize = 16;
    let l = net::TcpListener::bind("127.0.0.1:0").unwrap();
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
        .register(&mut s, Token(1), Interest::WRITABLE)
        .unwrap();
    let mut events = Events::with_capacity(16);

    // Wait for our TCP stream to connect
    'outer: loop {
        poll.poll(&mut events, None).unwrap();
        for event in events.iter() {
            if event.token() == Token(1) && event.is_writable() {
                break 'outer;
            }
        }
    }

    for _ in 0..N {
        s.write_all(&[1; 1024]).unwrap();
    }

    t.join().unwrap();
}

#[cfg(unix)]
#[test]
fn connection_reset_by_peer() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);
    let mut buf = [0u8; 16];

    // Create listener
    let mut l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = l.local_addr().unwrap();

    // Connect client
    let client = net2::TcpBuilder::new_v4().unwrap().to_tcp_stream().unwrap();

    client.set_linger(Some(Duration::from_millis(0))).unwrap();
    client.connect(&addr).unwrap();

    // Convert to Mio stream
    // FIXME: how to convert the stream on Windows?
    let mut client = unsafe { TcpStream::from_raw_fd(client.into_raw_fd()) };

    // Register server
    poll.registry()
        .register(&mut l, Token(0), Interest::READABLE)
        .unwrap();

    // Register interest in the client
    poll.registry()
        .register(
            &mut client,
            Token(1),
            Interest::READABLE | Interest::WRITABLE,
        )
        .unwrap();

    // Wait for listener to be ready
    let mut server;
    'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(0) {
                match l.accept() {
                    Ok((sock, _)) => {
                        server = sock;
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
    sleep(Duration::from_millis(100));

    // Register interest in the server socket
    poll.registry()
        .register(&mut server, Token(3), Interest::READABLE)
        .unwrap();

    loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(3) {
                assert!(event.is_readable());

                match server.read(&mut buf) {
                    Ok(0) | Err(_) => {}

                    Ok(x) => panic!("expected empty buffer but read {} bytes", x),
                }
                return;
            }
        }
    }
}

#[test]
fn connect_error() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);

    // Pick a "random" port that shouldn't be in use.
    let mut l = match TcpStream::connect("127.0.0.1:38381".parse().unwrap()) {
        Ok(l) => l,
        Err(ref e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            // Connection failed synchronously.  This is not a bug, but it
            // unfortunately doesn't get us the code coverage we want.
            return;
        }
        Err(e) => panic!("TcpStream::connect unexpected error {:?}", e),
    };

    poll.registry()
        .register(&mut l, Token(0), Interest::WRITABLE)
        .unwrap();

    'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(0) {
                assert!(event.is_writable());
                break 'outer;
            }
        }
    }

    assert!(l.take_error().unwrap().is_some());
}

#[test]
fn write_error() {
    init();

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(16);
    let (tx, rx) = channel();

    let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let t = thread::spawn(move || {
        let (conn, _addr) = listener.accept().unwrap();
        rx.recv().unwrap();
        drop(conn);
    });

    let mut s = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&mut s, Token(0), Interest::READABLE | Interest::WRITABLE)
        .unwrap();

    let mut wait_writable = || 'outer: loop {
        poll.poll(&mut events, None).unwrap();

        for event in &events {
            if event.token() == Token(0) && event.is_writable() {
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
                println!("good error: {}", e);
                break;
            }
        }
    }
}

macro_rules! wait {
    ($poll:ident, $ready:ident, $expect_read_closed: expr) => {{
        use std::time::Instant;

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
                if $expect_read_closed {
                    assert!(event.is_read_closed());
                } else {
                    assert!(!event.is_read_closed() && !event.is_write_closed());
                }

                if event.token() == Token(0) && event.$ready() {
                    found = true;
                    break;
                }
            }
        }
    }};
}

#[test]
fn write_shutdown() {
    init();

    let mut poll = Poll::new().unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let mut client = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(
            &mut client,
            Token(0),
            Interest::READABLE.add(Interest::WRITABLE),
        )
        .unwrap();

    let (socket, _) = listener.accept().unwrap();

    wait!(poll, is_writable, false);

    let mut events = Events::with_capacity(16);

    // Polling should not have any events
    poll.poll(&mut events, Some(Duration::from_millis(100)))
        .unwrap();

    let next = events.iter().next();
    assert!(next.is_none());

    println!("SHUTTING DOWN");
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
    init();

    let addr = "127.0.0.1:0".parse().unwrap();
    let mut server = TcpListener::bind(addr).unwrap();
    let addr = server.local_addr().unwrap();

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut server, LISTEN, Interest::READABLE)
        .unwrap();

    let mut sock = TcpStream::connect(addr).unwrap();
    poll.registry()
        .register(&mut sock, CLIENT, Interest::READABLE)
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
                LISTEN => {
                    let mut sock = handler.listener.accept().unwrap().0;
                    poll.registry()
                        .register(&mut sock, SERVER, Interest::WRITABLE)
                        .unwrap();
                    handler.accepted = Some(sock);
                }
                SERVER => {
                    handler.accepted.as_ref().unwrap().peer_addr().unwrap();
                    handler.accepted.as_ref().unwrap().local_addr().unwrap();
                    let n = handler
                        .accepted
                        .as_mut()
                        .unwrap()
                        .write(&[1, 2, 3])
                        .unwrap();
                    assert_eq!(n, 3);
                    handler.accepted = None;
                }
                CLIENT => {
                    handler.connected.peer_addr().unwrap();
                    handler.connected.local_addr().unwrap();
                    handler.shutdown = true;
                }
                _ => panic!("unexpected token"),
            }
        }
    }
}

#[test]
fn write_then_drop() {
    init();

    let mut a = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = a.local_addr().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();

    let mut poll = Poll::new().unwrap();

    poll.registry()
        .register(&mut a, Token(1), Interest::READABLE)
        .unwrap();

    poll.registry()
        .register(&mut s, Token(3), Interest::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(1));

    let mut s2 = a.accept().unwrap().0;

    poll.registry()
        .register(&mut s2, Token(2), Interest::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(2));

    s2.write_all(&[1, 2, 3, 4]).unwrap();
    drop(s2);

    poll.registry()
        .reregister(&mut s, Token(3), Interest::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(3));

    let mut buf = [0; 10];
    expect_read!(s.read(&mut buf), &[1, 2, 3, 4]);
}

#[test]
fn write_then_deregister() {
    init();

    let mut a = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = a.local_addr().unwrap();
    let mut s = TcpStream::connect(addr).unwrap();

    let mut poll = Poll::new().unwrap();

    poll.registry()
        .register(&mut a, Token(1), Interest::READABLE)
        .unwrap();
    poll.registry()
        .register(&mut s, Token(3), Interest::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(1));

    let mut s2 = a.accept().unwrap().0;

    poll.registry()
        .register(&mut s2, Token(2), Interest::WRITABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(2));

    s2.write_all(&[1, 2, 3, 4]).unwrap();
    poll.registry().deregister(&mut s2).unwrap();

    poll.registry()
        .reregister(&mut s, Token(3), Interest::READABLE)
        .unwrap();

    let mut events = Events::with_capacity(1024);
    while events.is_empty() {
        poll.poll(&mut events, None).unwrap();
    }
    assert_eq!(events.iter().count(), 1);
    assert_eq!(events.iter().next().unwrap().token(), Token(3));

    let mut buf = [0; 10];
    expect_read!(s.read(&mut buf), &[1, 2, 3, 4]);
}

const ID1: Token = Token(1);
const ID2: Token = Token(2);
const ID3: Token = Token(3);

#[test]
fn tcp_no_events_after_deregister() {
    let (mut poll, mut events) = init_with_poll();

    let mut listener = TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();
    let mut stream = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&mut listener, ID1, Interest::READABLE)
        .unwrap();
    poll.registry()
        .register(&mut stream, ID3, Interest::READABLE)
        .unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interest::READABLE)],
    );

    let (mut stream2, peer_address) = listener.accept().expect("unable to accept connection");
    assert!(peer_address.ip().is_loopback());
    assert_eq!(stream2.peer_addr().unwrap(), peer_address);
    assert_eq!(stream2.local_addr().unwrap(), addr);

    poll.registry()
        .register(&mut stream2, ID2, Interest::WRITABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID2, Interest::WRITABLE)],
    );

    stream2.write_all(&[1, 2, 3, 4]).unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID3, Interest::READABLE)],
    );

    poll.registry().deregister(&mut listener).unwrap();
    poll.registry().deregister(&mut stream).unwrap();
    poll.registry().deregister(&mut stream2).unwrap();

    expect_no_events(&mut poll, &mut events);

    let mut buf = [0; 10];
    expect_read!(stream.read(&mut buf), &[1, 2, 3, 4]);

    checked_write!(stream2.write(&[1, 2, 3, 4]));
    expect_no_events(&mut poll, &mut events);

    sleep(Duration::from_millis(200));
    expect_read!(stream.read(&mut buf), &[1, 2, 3, 4]);

    expect_no_events(&mut poll, &mut events);
}
