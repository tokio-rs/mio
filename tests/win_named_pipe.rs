#![cfg(all(windows, feature = "os-poll", feature = "os-ext"))]

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
use std::time::{Duration, Instant};

use mio::windows::NamedPipe;
use mio::{Events, Interest, Poll, Token};
use windows_sys::Win32::{Foundation::ERROR_NO_DATA, Storage::FileSystem::FILE_FLAG_OVERLAPPED};

mod util;
use util::{expect_events, ExpectEvent};

fn _assert_kinds() {
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}
    _assert_send::<NamedPipe>();
    _assert_sync::<NamedPipe>();
}

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {}", stringify!($e), e),
        }
    };
}

fn server() -> (NamedPipe, String) {
    let num: u64 = rand::random();
    let name = format!(r"\\.\pipe\my-pipe-{}", num);
    let pipe = t!(NamedPipe::new(&name));
    (pipe, name)
}

fn client(name: &str) -> NamedPipe {
    let mut opts = OpenOptions::new();
    opts.read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED);
    let file = t!(opts.open(name));
    unsafe { NamedPipe::from_raw_handle(file.into_raw_handle()) }
}

fn pipe() -> (NamedPipe, NamedPipe) {
    let (pipe, name) = server();
    (pipe, client(&name))
}

#[test]
fn writable_after_register() {
    let (mut server, mut client) = pipe();
    let mut poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::WRITABLE | Interest::READABLE,
    ));
    t!(poll
        .registry()
        .register(&mut client, Token(1), Interest::WRITABLE));

    let mut events = Events::with_capacity(128);
    t!(poll.poll(&mut events, None));

    let events = events.iter().collect::<Vec<_>>();
    assert!(events
        .iter()
        .any(|e| { e.token() == Token(0) && e.is_writable() }));
    assert!(events
        .iter()
        .any(|e| { e.token() == Token(1) && e.is_writable() }));
}

#[test]
fn write_then_read() {
    let (mut server, mut client) = pipe();
    let mut poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));
    t!(poll.registry().register(
        &mut client,
        Token(1),
        Interest::READABLE | Interest::WRITABLE,
    ));

    let mut events = Events::with_capacity(128);
    t!(poll.poll(&mut events, None));

    assert_eq!(t!(client.write(b"1234")), 4);

    loop {
        t!(poll.poll(&mut events, None));
        let events = events.iter().collect::<Vec<_>>();
        if let Some(event) = events.iter().find(|e| e.token() == Token(0)) {
            if event.is_readable() {
                break;
            }
        }
    }

    let mut buf = [0; 10];
    assert_eq!(t!(server.read(&mut buf)), 4);
    assert_eq!(&buf[..4], b"1234");
}

#[test]
fn connect_before_client() {
    let (mut server, name) = server();
    let mut poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));

    let mut events = Events::with_capacity(128);
    t!(poll.poll(&mut events, Some(Duration::new(0, 0))));
    assert_eq!(events.iter().count(), 0);
    assert_eq!(
        server.connect().err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );

    let mut client = client(&name);
    t!(poll.registry().register(
        &mut client,
        Token(1),
        Interest::READABLE | Interest::WRITABLE,
    ));
    loop {
        t!(poll.poll(&mut events, None));
        let e = events.iter().collect::<Vec<_>>();
        if let Some(event) = e.iter().find(|e| e.token() == Token(0)) {
            if event.is_writable() {
                break;
            }
        }
    }
}

#[test]
fn connect_after_client() {
    let (mut server, name) = server();
    let mut poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));

    let mut events = Events::with_capacity(128);
    t!(poll.poll(&mut events, Some(Duration::new(0, 0))));
    assert_eq!(events.iter().count(), 0);

    let mut client = client(&name);
    t!(poll.registry().register(
        &mut client,
        Token(1),
        Interest::READABLE | Interest::WRITABLE,
    ));
    t!(server.connect());
    loop {
        t!(poll.poll(&mut events, None));
        let e = events.iter().collect::<Vec<_>>();
        if let Some(event) = e.iter().find(|e| e.token() == Token(0)) {
            if event.is_writable() {
                break;
            }
        }
    }
}

#[test]
fn write_disconnected() {
    let mut poll = t!(Poll::new());
    let (mut server, mut client) = pipe();
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));
    t!(poll.registry().register(
        &mut client,
        Token(1),
        Interest::READABLE | Interest::WRITABLE,
    ));

    drop(client);

    let mut events = Events::with_capacity(128);
    t!(poll.poll(&mut events, None));
    assert!(events.iter().count() > 0);

    // this should not hang
    let mut i = 0;
    loop {
        i += 1;
        assert!(i < 16, "too many iterations");

        match server.write(&[0]) {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                t!(poll.poll(&mut events, None));
                assert!(events.iter().count() > 0);
            }
            Err(e) if e.raw_os_error() == Some(ERROR_NO_DATA as i32) => break,
            e => panic!("{:?}", e),
        }
    }
}

#[test]
fn write_then_drop() {
    let (mut server, mut client) = pipe();
    let mut poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));
    t!(poll.registry().register(
        &mut client,
        Token(1),
        Interest::READABLE | Interest::WRITABLE,
    ));
    assert_eq!(t!(client.write(b"1234")), 4);
    drop(client);

    let mut events = Events::with_capacity(128);

    'outer: loop {
        t!(poll.poll(&mut events, None));
        let events = events.iter().collect::<Vec<_>>();

        for event in &events {
            if event.is_readable() && event.token() == Token(0) {
                break 'outer;
            }
        }
    }

    let mut buf = [0; 10];
    assert_eq!(t!(server.read(&mut buf)), 4);
    assert_eq!(&buf[..4], b"1234");
}

#[test]
fn connect_twice() {
    let (mut server, name) = server();
    let mut c1 = client(&name);
    let mut poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));
    t!(poll
        .registry()
        .register(&mut c1, Token(1), Interest::READABLE | Interest::WRITABLE));
    drop(c1);

    let mut events = Events::with_capacity(128);

    'wait_for_eof: loop {
        'wait_for_readable: loop {
            t!(poll.poll(&mut events, None));
            let events = events.iter().collect::<Vec<_>>();
            for event in &events {
                if event.is_readable() && event.token() == Token(0) {
                    break 'wait_for_readable;
                }
            }
        }
        let mut buf = [0; 10];
        match server.read(&mut buf) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue 'wait_for_eof,
            Ok(0) => break 'wait_for_eof,
            res => panic!("{:?}", res),
        }
    }

    t!(server.disconnect());
    assert_eq!(
        server.connect().err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );

    let mut c2 = client(&name);
    t!(poll
        .registry()
        .register(&mut c2, Token(2), Interest::READABLE | Interest::WRITABLE));

    'outer: loop {
        t!(poll.poll(&mut events, None));
        let events = events.iter().collect::<Vec<_>>();

        for event in &events {
            if event.is_writable() && event.token() == Token(0) {
                break 'outer;
            }
        }
    }
}

#[test]
fn reregister_deregister_before_register() {
    let (mut pipe, _) = server();
    let poll = t!(Poll::new());

    assert_eq!(
        poll.registry()
            .reregister(&mut pipe, Token(0), Interest::READABLE)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound,
    );

    assert_eq!(
        poll.registry().deregister(&mut pipe).unwrap_err().kind(),
        io::ErrorKind::NotFound,
    );
}

#[test]
fn reregister_deregister_different_poll() {
    let (mut pipe, _) = server();
    let poll1 = t!(Poll::new());
    let poll2 = t!(Poll::new());

    // Register with 1
    t!(poll1
        .registry()
        .register(&mut pipe, Token(0), Interest::READABLE));

    assert_eq!(
        poll2
            .registry()
            .reregister(&mut pipe, Token(0), Interest::READABLE)
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists,
    );

    assert_eq!(
        poll2.registry().deregister(&mut pipe).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists,
    );
}

#[test]
fn read_message_larger_than_internal_buffer() {
    let (mut server, mut client) = pipe();
    let mut poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));
    t!(poll.registry().register(
        &mut client,
        Token(1),
        Interest::READABLE | Interest::WRITABLE,
    ));
    let mut events = Events::with_capacity(128);
    t!(poll.poll(&mut events, None));

    // Send message larger than the IPC kernel buffer (4096 bytes)
    let expected_msg = vec![0x5u8; 8192];
    assert_eq!(t!(client.write(&expected_msg)), 8192);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::READABLE)],
    );

    let mut buf = [0u8; 4000];
    let mut actual_msg = Vec::new();

    loop {
        match server.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                actual_msg.extend_from_slice(&buf[..n]);
                if actual_msg.len() >= expected_msg.len() {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                t!(poll.poll(&mut events, Some(Duration::from_secs(1))));
            }
            Err(e) => panic!("error reading message: {e}"),
        }
    }

    assert_eq!(expected_msg, actual_msg);
}

#[test]
fn read_with_small_buffer_provided() {
    let (mut server, mut client) = pipe();
    let mut poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));
    t!(poll.registry().register(
        &mut client,
        Token(1),
        Interest::READABLE | Interest::WRITABLE,
    ));

    let mut events = Events::with_capacity(128);
    t!(poll.poll(&mut events, None));

    let expected_msg = vec![1u8; 10000];
    assert_eq!(t!(client.write(&expected_msg)), 10000);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::READABLE)],
    );

    let mut buf = [0u8; 128];
    let mut actual_msg = Vec::new();

    loop {
        match server.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                actual_msg.extend_from_slice(&buf[..n]);
                if actual_msg.len() >= expected_msg.len() {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                t!(poll.poll(&mut events, Some(Duration::from_millis(100))));
            }
            Err(e) => panic!("error reading message: {e}"),
        }
    }

    assert_eq!(actual_msg, expected_msg);
}

#[test]
fn handle_closed_on_drop() {
    const TIMEOUT: Duration = Duration::from_secs(2);

    let (mut server, mut client) = pipe();
    let mut server_poll = t!(Poll::new());

    t!(server_poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));
    t!(server.connect());

    {
        // Create another Poll as if we are in a separate process running a separate event loop.
        let client_poll = t!(Poll::new());
        t!(client_poll.registry().register(
            &mut client,
            Token(1),
            Interest::READABLE | Interest::WRITABLE,
        ));

        let mut spam = b"spam".to_vec();
        spam.resize(1024 * 1024, 0); // 1MiB to make sure it blocks on server-side read

        // first write should not return WouldBlock
        t!(client.write(&spam));
        // now there's an OVERLAPPED that will hold a ref to Arc<Inner> indefinitely

        // order and presence of these 3 lines makes no difference:
        let _ = client_poll.registry().deregister(&mut client);
        drop(client);
        drop(client_poll);
        // Either way, client_poll will get dropped by the end of this block.
        // Inside the drop, client_poll will make a (vain) attempt to drain the IOCP of all events
        // and release all references to `client` named pipe.
        // But the large write we just submited will not complete in this timeframe, and there will be
        // one reference to `client.inner` that will never get released.
        //
        // Doing a large write is the reliable way to reproduce this, but writing thru `server` pipe in busy loop
        // in a separate process and reading from `client` can also lead to leaked handles (race condition
        // between CancelIoEx and GetQueuedCompletionStatusEx).
    }

    // As server, read until eof.
    // Since client's NamedPipe and even Poll got dropped, we should eventually get an EOF.
    let mut events = Events::with_capacity(128);
    let mut buf = vec![0; 1024];
    let start_time = Instant::now();
    'wait_for_eof: loop {
        t!(server_poll.poll(&mut events, Some(TIMEOUT)));
        if events.is_empty() {
            panic!(
                "timed out waiting for eof after {}ms",
                start_time.elapsed().as_millis()
            );
        }
        'drain: loop {
            match server.read(&mut buf) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    break 'drain;
                }
                Err(_) | Ok(0) => break 'wait_for_eof,
                Ok(_) => (),
            }
        }
    }
}
