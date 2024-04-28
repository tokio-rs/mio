#![cfg(all(windows, feature = "os-poll", feature = "os-ext"))]

use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{FromRawHandle, IntoRawHandle, RawHandle};
use std::time::Duration;

use mio::windows::NamedPipe;
use mio::{Events, Interest, Poll, Token};
use rand::Rng;
use windows_sys::Win32::Foundation::ERROR_NO_DATA;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES,
};

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
    let num: u64 = rand::thread_rng().gen();
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

fn pipe_msg_mode() -> (NamedPipe, NamedPipe) {
    let num: u64 = rand::thread_rng().gen();
    let name = format!(r"\\.\pipe\my-pipe-{}", num);
    let name: Vec<_> = OsStr::new(&name).encode_wide().chain(Some(0)).collect();
    unsafe {
        let h = CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE,
            PIPE_UNLIMITED_INSTANCES,
            65536,
            65536,
            0,
            std::ptr::null_mut(),
        );

        let server = NamedPipe::from_raw_handle(h as RawHandle);

        let h = CreateFileW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            0,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            0,
        );
        let client = NamedPipe::from_raw_handle(h as RawHandle);
        (server, client)
    }
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
fn read_sz_greater_than_default_buf_size() {
    let (mut server, mut client) = pipe_msg_mode();
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
    let msg = (0..4106)
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("");

    t!(poll.poll(&mut events, None));
    assert_eq!(t!(client.write(msg.as_bytes())), 15314);

    loop {
        t!(poll.poll(&mut events, None));
        let events = events.iter().collect::<Vec<_>>();
        if let Some(event) = events.iter().find(|e| e.token() == Token(0)) {
            if event.is_readable() {
                break;
            }
        }
    }

    let mut buf = [0; 15314];
    assert_eq!(t!(server.read(&mut buf)), 15314);
    assert_eq!(&buf[..15314], msg.as_bytes());
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

    loop {
        t!(poll.poll(&mut events, None));
        let events = events.iter().collect::<Vec<_>>();
        if let Some(event) = events.iter().find(|e| e.token() == Token(0)) {
            if event.is_readable() {
                let mut buf = [0; 10];

                match server.read(&mut buf) {
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Ok(0) => break,
                    res => panic!("{:?}", res),
                }
            }
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
