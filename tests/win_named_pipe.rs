#![cfg(all(windows, feature = "os-poll", feature = "os-ext"))]

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use std::{mem, panic, ptr, thread};

use mio::windows::NamedPipe;
use mio::{Events, Interest, Poll, Registry, Token, Waker};
use windows_sys::Win32::Foundation::{
    GetHandleInformation, ERROR_ACCESS_DENIED, ERROR_NO_DATA, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, SetNamedPipeHandleState, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE,
    PIPE_UNLIMITED_INSTANCES,
};

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

/// Like `server`, but in message mode.
fn message_server() -> (NamedPipe, String) {
    let num: u64 = rand::random();
    let name = format!(r"\\.\pipe\my-pipe-{}", num);
    let wide: Vec<u16> = OsStr::new(&name).encode_wide().chain(Some(0)).collect();
    let handle = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE,
            PIPE_UNLIMITED_INSTANCES,
            65536,
            65536,
            0,
            ptr::null(),
        )
    };
    assert!(
        handle != INVALID_HANDLE_VALUE,
        "{}",
        io::Error::last_os_error()
    );
    let pipe = unsafe { NamedPipe::from_raw_handle(handle) };
    (pipe, name)
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
fn write_vectored_then_read() {
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

    let bufs = [
        IoSlice::new(b"12"),
        IoSlice::new(b""),
        IoSlice::new(b"345"),
        IoSlice::new(b"6789"),
    ];
    assert_eq!(t!(client.write_vectored(&bufs)), 9);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::READABLE)],
    );

    let mut buf = [0; 16];
    assert_eq!(t!(server.read(&mut buf)), 9);
    assert_eq!(&buf[..9], b"123456789");
}

#[test]
fn write_then_read_vectored() {
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

    assert_eq!(t!(client.write(b"123456789")), 9);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::READABLE)],
    );

    let mut buf1 = [0; 2];
    let mut buf2 = [0; 0];
    let mut buf3 = [0; 3];
    let mut buf4 = [0; 8];
    let mut bufs = [
        IoSliceMut::new(&mut buf1),
        IoSliceMut::new(&mut buf2),
        IoSliceMut::new(&mut buf3),
        IoSliceMut::new(&mut buf4),
    ];
    assert_eq!(t!(server.read_vectored(&mut bufs)), 9);
    assert_eq!(&buf1, b"12");
    assert_eq!(&buf3, b"345");
    assert_eq!(&buf4[..4], b"6789");
}

#[test]
fn read_vectored_keeps_remainder() {
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

    assert_eq!(t!(client.write(b"123456789")), 9);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::READABLE)],
    );

    let mut buf1 = [0; 2];
    let mut buf2 = [0; 0];
    let mut buf3 = [0; 3];
    let mut bufs = [
        IoSliceMut::new(&mut buf1),
        IoSliceMut::new(&mut buf2),
        IoSliceMut::new(&mut buf3),
    ];
    assert_eq!(t!(server.read_vectored(&mut bufs)), 5);
    assert_eq!(&buf1, b"12");
    assert_eq!(&buf3, b"345");

    // The rest is kept for the next read.
    let mut buf = [0; 16];
    assert_eq!(t!(server.read(&mut buf)), 4);
    assert_eq!(&buf[..4], b"6789");
    match server.read(&mut buf) {
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
        res => panic!("expected the data to be used up, got {res:?}"),
    }
}

// A vectored write is sent as a single message.
#[test]
fn write_vectored_message_mode() {
    let (mut server, name) = message_server();
    let mut client = blocking_client(&name);
    // Read whole messages, so a split write shows up as a short read.
    let mode = PIPE_READMODE_MESSAGE;
    let ok =
        unsafe { SetNamedPipeHandleState(client.as_raw_handle(), &mode, ptr::null(), ptr::null()) };
    assert!(ok != 0, "{}", io::Error::last_os_error());

    let mut poll = t!(Poll::new());
    t!(poll
        .registry()
        .register(&mut server, Token(0), Interest::WRITABLE));
    let mut events = Events::with_capacity(16);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::WRITABLE)],
    );

    let bufs = [
        IoSlice::new(b"12"),
        IoSlice::new(b""),
        IoSlice::new(b"345"),
        IoSlice::new(b"6789"),
    ];
    assert_eq!(t!(server.write_vectored(&bufs)), 9);

    let mut buf = [0; 16];
    assert_eq!(t!(client.read(&mut buf)), 9);
    assert_eq!(&buf[..9], b"123456789");
}

// A message larger than the internal buffer arrives in pieces, all but the last
// read completing with `ERROR_MORE_DATA`, whether the message is sent before
// or after the read is submitted.
#[test]
fn read_message_mode_larger_than_internal_buffer() {
    for send_first in [false, true] {
        let (mut server, name) = message_server();
        let mut client = blocking_client(&name);
        let mut poll = t!(Poll::new());
        let msg: Vec<u8> = (0..10000).map(|i| i as u8).collect();
        let mut send = || {
            assert_eq!(t!(client.write(&msg)), msg.len());
        };
        if send_first {
            send();
        }
        t!(poll
            .registry()
            .register(&mut server, Token(0), Interest::READABLE));
        if !send_first {
            send();
        }
        // Turns a lost byte into an early EOF.
        drop(client);

        let mut events = Events::with_capacity(16);
        let mut buf = [0; 8192];
        let mut read = Vec::new();
        let timeout = Instant::now() + Duration::from_secs(10);
        loop {
            match server.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => read.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    t!(poll.poll(&mut events, Some(Duration::from_millis(100))));
                }
                Err(e) => panic!("unexpected error: {e}"),
            }
            assert!(Instant::now() < timeout, "EOF never delivered");
        }
        assert!(read == msg, "got {} of {} bytes", read.len(), msg.len());
    }
}

// Once the size of the caller's buffer is known, reads from the pipe use it.
#[test]
fn read_with_large_buffer() {
    for vectored in [false, true] {
        let (mut server, name) = server();
        let mut client = blocking_client(&name);
        let mut poll = t!(Poll::new());
        t!(poll
            .registry()
            .register(&mut server, Token(0), Interest::READABLE));

        // Returns once all of it is read or in the pipe's buffer.
        let data: Vec<u8> = (0..64 * 1024).map(|i| (i % 251) as u8).collect();
        t!(client.write_all(&data));

        // The first read was submitted on register, at the default size. The
        // next one is sized like `buf` and gets all the rest.
        let mut buf = vec![0; 64 * 1024];
        let mut read = Vec::new();
        for _ in 0..2 {
            let n = read_when_ready(&mut poll, || {
                if vectored {
                    let (a, b) = buf.split_at_mut(32 * 1024);
                    server.read_vectored(&mut [IoSliceMut::new(a), IoSliceMut::new(b)])
                } else {
                    server.read(&mut buf)
                }
            });
            read.extend_from_slice(&buf[..n]);
        }
        assert!(read == data, "got {} of {} bytes", read.len(), data.len());
    }
}

// The read size follows the caller's buffer as it changes, and data read for a
// larger buffer is kept for the next calls.
#[test]
fn read_buffer_size_changes() {
    let (mut server, name) = server();
    let mut client = blocking_client(&name);
    let mut poll = t!(Poll::new());
    t!(poll
        .registry()
        .register(&mut server, Token(0), Interest::READABLE));
    let data: Vec<u8> = (0..64 * 1024).map(|i| (i % 251) as u8).collect();
    let mut buf = vec![0; 64 * 1024];
    let mut read = Vec::new();

    t!(client.write_all(&data));
    // Submitted on register, at the default size.
    let n = read_when_ready(&mut poll, || server.read(&mut buf));
    read.extend_from_slice(&buf[..n]);
    // The next read is sized like `buf` and gets all the rest, which smaller
    // buffers then take in parts.
    for len in [1024, 16 * 1024] {
        let n = read_when_ready(&mut poll, || server.read(&mut buf[..len]));
        assert_eq!(n, len);
        read.extend_from_slice(&buf[..n]);
    }
    while read.len() < data.len() {
        let n = read_when_ready(&mut poll, || server.read(&mut buf[..1024]));
        read.extend_from_slice(&buf[..n]);
    }
    assert!(read == data, "got {} of {} bytes", read.len(), data.len());

    // The last 1 KiB read sized the next one down to the minimum, the one
    // after it is sized like `buf` again.
    read.clear();
    t!(client.write_all(&data));
    let n = read_when_ready(&mut poll, || server.read(&mut buf));
    assert_eq!(n, 4096);
    read.extend_from_slice(&buf[..n]);
    let n = read_when_ready(&mut poll, || server.read(&mut buf));
    read.extend_from_slice(&buf[..n]);
    assert!(read == data, "got {} of {} bytes", read.len(), data.len());
}

// In message mode, a message larger than the default read size arrives in one
// read once the size of the caller's buffer is known.
#[test]
fn read_message_mode_with_large_buffer() {
    let (mut server, name) = message_server();
    let mut client = blocking_client(&name);
    let mut poll = t!(Poll::new());
    t!(poll
        .registry()
        .register(&mut server, Token(0), Interest::READABLE));
    let first: Vec<u8> = (0..10000).map(|i| (i % 251) as u8).collect();
    let second: Vec<u8> = (0..10000).map(|i| (i % 241) as u8).collect();
    for msg in [&first, &second] {
        assert_eq!(t!(client.write(msg)), msg.len());
    }

    // The first message arrives in parts: the first read was submitted on
    // register, at the default size.
    let mut buf = vec![0; 16 * 1024];
    let mut read = Vec::new();
    while read.len() < first.len() {
        let n = read_when_ready(&mut poll, || server.read(&mut buf));
        read.extend_from_slice(&buf[..n]);
    }
    assert!(read == first, "got {} of {} bytes", read.len(), first.len());
    // The second one in a single read.
    let n = read_when_ready(&mut poll, || server.read(&mut buf));
    assert_eq!(n, second.len());
    assert!(buf[..n] == second[..]);
}

/// Calls `read` until it returns data, polling while it would block.
fn read_when_ready(poll: &mut Poll, mut read: impl FnMut() -> io::Result<usize>) -> usize {
    let mut events = Events::with_capacity(16);
    let timeout = Instant::now() + Duration::from_secs(10);
    loop {
        match read() {
            Ok(0) => panic!("unexpected EOF"),
            Ok(n) => return n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                t!(poll.poll(&mut events, Some(Duration::from_millis(100))));
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
        assert!(Instant::now() < timeout, "no data to read");
    }
}

// A pending write that fails is reported as writable, then as the error.
#[test]
fn write_fails_after_submit() {
    let (mut server, name) = server();
    let client = blocking_client(&name);
    let mut poll = t!(Poll::new());
    t!(poll
        .registry()
        .register(&mut server, Token(0), Interest::WRITABLE));
    let mut events = Events::with_capacity(16);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::WRITABLE)],
    );

    // Larger than the pipe's buffer, so the write pends until the client goes.
    let data = vec![0; 128 * 1024];
    assert_eq!(t!(server.write(&data)), data.len());
    drop(client);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::WRITABLE)],
    );
    match server.write(&data) {
        Err(e) if e.kind() != io::ErrorKind::WouldBlock => {}
        res => panic!("expected the write's error, got {res:?}"),
    }
}

const WAKE: Token = Token(1);

/// Polls on a dedicated thread, blocking until a completion or wake-up arrives.
/// A separate peer thread disconnects clients, so polling can race with both
/// the disconnect and the submission, rather than waiting for the close to return.
struct Driver {
    clients: Option<mpsc::Sender<(File, Vec<u8>)>>,
    waker: Waker,
    wakes: mpsc::Receiver<()>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    peer: Option<thread::JoinHandle<()>>,
}

impl Driver {
    fn new(mut poll: Poll) -> Driver {
        let waker = t!(Waker::new(poll.registry(), WAKE));
        let (clients, rx) = mpsc::channel::<(File, Vec<u8>)>();
        let peer = thread::spawn(move || {
            for (mut client, after) in rx {
                t!(client.write_all(&after));
                drop(client);
            }
        });
        let (done, wakes) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = thread::spawn(move || {
            let mut events = Events::with_capacity(16);
            while !stopped.load(Relaxed) {
                t!(poll.poll(&mut events, None));
                for event in &events {
                    if event.token() == WAKE {
                        let _ = done.send(());
                    }
                }
            }
        });
        Driver {
            clients: Some(clients),
            waker,
            wakes,
            stop,
            thread: Some(thread),
            peer: Some(peer),
        }
    }

    fn disconnect(&self, client: File, after: Vec<u8>) {
        assert!(
            self.clients.as_ref().unwrap().send((client, after)).is_ok(),
            "peer thread panicked"
        );
    }

    /// Waits until everything queued so far has been handled: the wake-up is
    /// queued after it.
    fn sync(&self) {
        assert!(
            !self.peer.as_ref().unwrap().is_finished(),
            "peer thread panicked"
        );
        t!(self.waker.wake());
        match self.wakes.recv_timeout(Duration::from_secs(10)) {
            Ok(()) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => panic!("poll thread stuck"),
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("poll thread panicked"),
        }
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        drop(self.clients.take());
        self.stop.store(true, Relaxed);
        let _ = self.waker.wake();
        // If the test is already failing don't wait, the thread may be stuck in
        // broken code.
        if !thread::panicking() {
            for thread in [self.peer.take().unwrap(), self.thread.take().unwrap()] {
                if let Err(err) = thread.join() {
                    panic::resume_unwind(err);
                }
            }
        }
    }
}

/// Runs `round` on three threads, each with its own driver, until `budget`
/// runs out. The races need some contention, parallel rounds provide it.
fn race(budget: Duration, round: fn(&Registry, &Driver)) {
    let threads: Vec<_> = (0..3)
        .map(|_| {
            thread::spawn(move || {
                let poll = t!(Poll::new());
                let registry = t!(poll.registry().try_clone());
                let driver = Driver::new(poll);
                let deadline = Instant::now() + budget;
                while Instant::now() < deadline {
                    round(&registry, &driver);
                }
            })
        })
        .collect();
    for thread in threads {
        if let Err(err) = thread.join() {
            panic::resume_unwind(err);
        }
    }
}

fn blocking_client(name: &str) -> File {
    t!(OpenOptions::new().read(true).write(true).open(name))
}

/// Reads until EOF, letting `driver` catch up whenever the read would block.
fn read_to_end(server: &mut NamedPipe, driver: &Driver) -> Vec<u8> {
    let timeout = Instant::now() + Duration::from_secs(10);
    let mut buf = [0; 8192];
    let mut read = Vec::new();
    loop {
        match server.read(&mut buf) {
            Ok(0) => return read,
            Ok(n) => read.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => driver.sync(),
            Err(e) => panic!("unexpected error: {e}"),
        }
        assert!(Instant::now() < timeout, "EOF never delivered");
    }
}

/// Waits until the pipe `name` is gone, i.e. no reference to it leaked. Every
/// server is its first instance, so creating it again fails while it exists.
fn wait_freed(name: &str, driver: &Driver) {
    let timeout = Instant::now() + Duration::from_secs(10);
    loop {
        match NamedPipe::new(name) {
            Ok(_) => return,
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
        assert!(Instant::now() < timeout, "pipe leaked");
        driver.sync();
    }
}

// Issue #2011: the peer disconnects right as a read is resubmitted, and the
// driver dequeues the failed completion before `read` returns. Its callback
// then redeems a reference that was never reserved, freeing the pipe, and so
// closing its handle, while `server` still uses it.
#[test]
fn read_disconnect_race() {
    race(Duration::from_secs(3), |registry, driver| {
        let (mut server, name) = server();
        let handle = server.as_raw_handle();
        let mut client = blocking_client(&name);
        t!(registry.register(&mut server, Token(0), Interest::READABLE));
        t!(client.write_all(&[1]));
        // Handle the byte's completion, so the next `read` consumes it and
        // resubmits.
        driver.sync();
        driver.disconnect(client, Vec::new());
        assert_eq!(read_to_end(&mut server, driver), [1]);

        driver.sync();
        let mut flags = 0;
        if unsafe { GetHandleInformation(handle, &mut flags) } == 0 {
            let err = io::Error::last_os_error();
            // Don't touch the freed pipe again.
            mem::forget(server);
            panic!("pipe freed while still in use: {err}");
        }
        drop(server);
        wait_freed(&name, driver);
    });
}

// Same race for writes: the failed completion then finds no write in flight.
#[test]
fn write_disconnect_race() {
    race(Duration::from_secs(3), |registry, driver| {
        let (mut server, name) = server();
        let client = blocking_client(&name);
        t!(registry.register(&mut server, Token(0), Interest::WRITABLE));
        driver.sync();
        // Larger than the pipe's buffer, so the write pends. The random size
        // varies how long copying it takes, and so when the write is submitted.
        let data = vec![0; rand::random_range(66_000..130_000)];
        driver.disconnect(client, Vec::new());

        let timeout = Instant::now() + Duration::from_secs(10);
        loop {
            match server.write(&data) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => driver.sync(),
                Err(_) => break,
            }
            assert!(Instant::now() < timeout, "write never failed");
        }
        drop(server);
        wait_freed(&name, driver);
    });
}

// A message arrives right as a read is submitted, and the driver handles its
// `ERROR_MORE_DATA` completion before the submit returns. Treating that as a
// failed submit loses the data.
#[test]
fn read_more_data_race() {
    race(Duration::from_secs(1), |registry, driver| {
        let msg: Vec<u8> = (0..6000).map(|i| i as u8).collect();
        let (mut server, name) = message_server();
        let client = blocking_client(&name);
        // Sent while `register` submits the first read.
        driver.disconnect(client, msg.clone());
        t!(registry.register(&mut server, Token(0), Interest::READABLE));
        let read = read_to_end(&mut server, driver);
        assert!(read == msg, "got {} of {} bytes", read.len(), msg.len());
        drop(server);
        wait_freed(&name, driver);
    });
}
