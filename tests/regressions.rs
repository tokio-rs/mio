#![cfg(not(target_os = "wasi"))]
#![cfg(all(feature = "os-poll", feature = "net"))]

use std::io::{self, Read};
use std::sync::Arc;
use std::time::Duration;
use std::{net, thread};

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token, Waker};

mod util;
use util::{any_local_address, init, init_with_poll};

const ID1: Token = Token(1);
const WAKE_TOKEN: Token = Token(10);

#[test]
fn issue_776() {
    init();

    let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = thread::spawn(move || {
        let mut stream = listener.accept().expect("accept").0;
        // SO_RCVTIMEO not supported on GNU/Hurd
        #[cfg(not(target_os = "hurd"))]
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set_read_timeout");
        let _ = stream.read(&mut [0; 16]).expect("read");
    });

    let mut poll = Poll::new().unwrap();
    let mut stream = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(
            &mut stream,
            Token(1),
            Interest::READABLE | Interest::WRITABLE,
        )
        .unwrap();
    let mut events = Events::with_capacity(16);
    'outer: loop {
        poll.poll(&mut events, None).unwrap();
        for event in &events {
            if event.token() == Token(1) {
                // connected
                break 'outer;
            }
        }
    }

    let mut buf = [0; 1024];
    match stream.read(&mut buf) {
        Ok(_) => panic!("unexpected ok"),
        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => (),
        Err(err) => panic!("unexpected error: {err}"),
    }

    drop(stream);
    handle.join().unwrap();
}

#[test]
fn issue_1205() {
    let (mut poll, mut events) = init_with_poll();

    let waker = Arc::new(Waker::new(poll.registry(), WAKE_TOKEN).unwrap());

    // `_waker` must stay in scope in order for `Waker` events to be delivered
    // when the test polls for events. If it is not cloned, it is moved out of
    // scope in `thread::spawn` and `Poll::poll` will timeout.
    #[allow(clippy::redundant_clone)]
    let _waker = waker.clone();

    let mut listener = TcpListener::bind(any_local_address()).unwrap();

    poll.registry()
        .register(&mut listener, ID1, Interest::READABLE)
        .unwrap();

    poll.poll(&mut events, Some(std::time::Duration::from_millis(0)))
        .unwrap();
    assert!(events.iter().count() == 0);

    let _stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();

    poll.registry().deregister(&mut listener).unwrap();

    // spawn a waker thread to wake the poll call below
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        waker.wake().expect("unable to wake");
    });

    poll.poll(&mut events, None).unwrap();

    // the poll should return only one event that being the waker event.
    // the poll should not retrieve event for the listener above because it was
    // deregistered
    assert!(events.iter().count() == 1);
    let waker_event = events.iter().next().unwrap();
    assert!(waker_event.is_readable());
    assert_eq!(waker_event.token(), WAKE_TOKEN);
    handle.join().unwrap();
}

#[test]
#[cfg(unix)]
#[cfg_attr(miri, ignore = "Miri doesn't support Unix domain sockets")]
fn issue_1403() {
    use mio::net::UnixDatagram;
    use util::temp_file;

    init();

    let path = temp_file("issue_1403");
    let datagram1 = UnixDatagram::bind(&path).unwrap();
    let datagram2 = UnixDatagram::unbound().unwrap();

    let mut buf = [1u8; 1024];
    let n = datagram2.send_to(&buf, &path).unwrap();

    let (got, addr) = datagram1.recv_from(&mut buf).unwrap();
    assert_eq!(got, n);
    assert_eq!(addr.as_pathname(), None);
}

#[test]
#[cfg(all(windows, feature = "os-ext"))]
fn issue_1983() {
    use std::fs::OpenOptions;
    use std::io::Write;

    use mio::windows::NamedPipe;

    let (mut poll, mut events) = init_with_poll();
    let name = format!(r"\\.\pipe\mio-issue-1893-{}", rand::random::<u64>());
    let mut pipe = NamedPipe::new(&name).unwrap();

    poll.registry()
        .register(&mut pipe, Token(0), Interest::READABLE | Interest::WRITABLE)
        .unwrap();

    // Two rounds of Opening the pipe, opening a client that writes a message and then reading that message from the pipe
    // The two rounds should behave the same way
    for round in 0..2 {
        // Connect to the pipe, no clients are connected so this should return `WouldBlock`.
        use crate::util::assert_would_block;
        assert_would_block(pipe.connect());

        let mut client = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&name)
            .unwrap();
        let message = format!("round-{round}");
        client.write_all(message.as_bytes()).unwrap();
        drop(client);

        loop {
            poll.poll(&mut events, None).unwrap();
            if events
                .iter()
                .any(|event| event.token() == Token(0) && event.is_readable())
            {
                break;
            }
        }

        let mut buf = [0; 64];
        let n = loop {
            match pipe.read(&mut buf) {
                Ok(n) if n > 0 => break n,
                // EOF from the previous round leaked
                Ok(0) => panic!("received EOF before the round {round} message"),
                Ok(_) => unreachable!(),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    poll.poll(&mut events, None).unwrap();
                }
                Err(error) => panic!("read round {round} message: {error}"),
            }
        };
        assert_eq!(&buf[..n], message.as_bytes());

        loop {
            match pipe.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => panic!("unexpected {n} bytes after round {round} message"),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    poll.poll(&mut events, None).unwrap();
                }
                Err(error) => panic!("read round {round} EOF: {error}"),
            }
        }

        // Disconnect the pipe to prepare for the next round, when the pipe should behave as new
        pipe.disconnect().unwrap();
    }
}

/// Similar to `issue_1983` but the server does not read the EOF from the first message before disconnecting.
#[test]
#[cfg(all(windows, feature = "os-ext"))]
fn issue_1983_2() {
    use crate::util::assert_would_block;
    use mio::windows::NamedPipe;
    use std::fs::OpenOptions;
    use std::io::Write;

    let (mut poll, mut events) = init_with_poll();
    let name = format!(r"\\.\pipe\mio-issue-1983-2-{}", rand::random::<u64>());
    let mut pipe = NamedPipe::new(&name).unwrap();

    poll.registry()
        .register(&mut pipe, Token(0), Interest::READABLE | Interest::WRITABLE)
        .unwrap();

    assert_would_block(pipe.connect());

    let mut first_client = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&name)
        .unwrap();
    first_client.write_all(b"first").unwrap();

    let mut buf = [0; 64];
    loop {
        poll.poll(&mut events, None).unwrap();
        match pipe.read(&mut buf) {
            Ok(5) => break,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            result => panic!("read first message: {result:?}"),
        }
    }
    assert_eq!(&buf[..5], b"first");

    // Reading the entire first message scheduled another overlapped read.
    // Keep the client open so that read is still pending when the pipe disconnects.
    pipe.disconnect().unwrap();
    drop(first_client);

    assert_would_block(pipe.connect());

    let mut second_client = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&name)
        .unwrap();
    second_client.write_all(b"second").unwrap();

    loop {
        poll.poll(&mut events, None).unwrap();
        match pipe.read(&mut buf) {
            Ok(6) => break,
            Ok(0) => panic!("read stale EOF before second message"),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            // Read result from before second connect
            Err(error) => panic!(
                "stale read error before second message: {error} ({:?})",
                error.raw_os_error()
            ),
            result => panic!("read second message: {result:?}"),
        }
    }
    assert_eq!(&buf[..6], b"second");
}

/// Similar to `issue_1983` but this time, the server never reads the first message, so the
/// completed read is buffered in `State::Ok` rather than left pending when the
/// pipe disconnects.
#[test]
#[cfg(all(windows, feature = "os-ext"))]
fn issue_1983_3() {
    use std::fs::OpenOptions;
    use std::io::Write;

    use crate::util::assert_would_block;
    use mio::windows::NamedPipe;

    let (mut poll, mut events) = init_with_poll();
    let name = format!(r"\\.\pipe\mio-issue-1983-3-{}", rand::random::<u64>());
    let mut pipe = NamedPipe::new(&name).unwrap();

    poll.registry()
        .register(&mut pipe, Token(0), Interest::READABLE | Interest::WRITABLE)
        .unwrap();

    assert_would_block(pipe.connect());

    let mut first_client = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&name)
        .unwrap();
    first_client.write_all(b"first").unwrap();
    drop(first_client);

    // Wait for the readable notification, which means `read_done` has already
    // buffered the first client's bytes. Deliberately don't read them.
    loop {
        poll.poll(&mut events, Some(Duration::from_secs(5)))
            .unwrap();
        if events
            .iter()
            .any(|event| event.token() == Token(0) && event.is_readable())
        {
            break;
        }
    }

    pipe.disconnect().unwrap();

    assert_would_block(pipe.connect());

    let mut second_client = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&name)
        .unwrap();
    second_client.write_all(b"second").unwrap();

    let mut buf = [0; 64];
    let n = loop {
        match pipe.read(&mut buf) {
            Ok(n) if n > 0 => break n,
            Ok(0) => panic!("read stale EOF before second message"),
            Ok(_) => unreachable!(),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                poll.poll(&mut events, Some(Duration::from_secs(5)))
                    .unwrap();
            }
            Err(error) => panic!("read second message: {error}"),
        }
    };
    // Buffered data from the first client must not be handed to the second.
    assert_eq!(&buf[..n], b"second");
}

/// Same staleness as `issue_1983` but on the write half: the first client goes
/// away while a write is in flight, so `write_done` parks its error in the pipe
/// and the next client is handed that error.
#[test]
#[cfg(all(windows, feature = "os-ext"))]
fn issue_1983_4() {
    use std::fs::OpenOptions;
    use std::io::Write;

    use crate::util::assert_would_block;
    use mio::windows::NamedPipe;

    let (mut poll, mut events) = init_with_poll();
    let name = format!(r"\\.\pipe\mio-issue-1983-4-{}", rand::random::<u64>());
    let mut pipe = NamedPipe::new(&name).unwrap();

    poll.registry()
        .register(&mut pipe, Token(0), Interest::READABLE | Interest::WRITABLE)
        .unwrap();

    assert_would_block(pipe.connect());

    let first_client = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&name)
        .unwrap();

    loop {
        poll.poll(&mut events, Some(Duration::from_secs(5))).unwrap();
        if events
            .iter()
            .any(|event| event.token() == Token(0) && event.is_writable())
        {
            break;
        }
    }

    // Larger than the pipe's 64 KiB outbound buffer, so the write stays in
    // flight instead of completing into the kernel buffer.
    let payload = vec![0xab; 65536 + 1];
    assert_eq!(pipe.write(&payload).unwrap(), payload.len());

    // The client never drains the pipe, so the in-flight write fails here.
    drop(first_client);

    pipe.disconnect().unwrap();

    assert_would_block(pipe.connect());

    let _second_client = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&name)
        .unwrap();

    loop {
        poll.poll(&mut events, Some(Duration::from_secs(5))).unwrap();
        if events
            .iter()
            .any(|event| event.token() == Token(0) && event.is_writable())
        {
            break;
        }
    }

    match pipe.write(b"second") {
        Ok(n) => assert_eq!(n, 6),
        Err(error) => panic!(
            "stale write error from the first client: {error} ({:?})",
            error.raw_os_error()
        ),
    }
}
