#![cfg(all(windows, feature = "os-poll", feature = "os-ext"))]

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
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
    CreateNamedPipeW, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES,
};

mod util;
use util::{assert_would_block, expect_events, ExpectEvent};

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

    // Don't wait for the readable event: it can be behind a writable one for
    // the same token, in the same poll.
    assert!(read_to_eof(&mut server, &mut poll, &mut events).is_empty());

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

/// Waits until the pipe `name` is gone, i.e. no reference to it leaked, calling
/// `sync` to let completions be handled. Every server is its first instance, so
/// creating it again fails while it exists.
fn wait_freed(name: &str, mut sync: impl FnMut()) {
    let timeout = Instant::now() + Duration::from_secs(10);
    loop {
        match NamedPipe::new(name) {
            Ok(_) => return,
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
        assert!(Instant::now() < timeout, "pipe leaked");
        sync();
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
        wait_freed(&name, || driver.sync());
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
        wait_freed(&name, || driver.sync());
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
        wait_freed(&name, || driver.sync());
    });
}

/// Reads until EOF, polling whenever the read would block.
fn read_to_eof(pipe: &mut NamedPipe, poll: &mut Poll, events: &mut Events) -> Vec<u8> {
    let timeout = Instant::now() + Duration::from_secs(10);
    let mut buf = [0; 8192];
    let mut read = Vec::new();
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => return read,
            Ok(n) => read.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                t!(poll.poll(events, Some(Duration::from_millis(100))));
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
        assert!(Instant::now() < timeout, "EOF never delivered");
    }
}

/// Reads the first data that arrives, polling whenever the read would block.
fn read_some(pipe: &mut NamedPipe, poll: &mut Poll, events: &mut Events) -> Vec<u8> {
    let timeout = Instant::now() + Duration::from_secs(10);
    let mut buf = [0; 8192];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => panic!("unexpected EOF"),
            Ok(n) => return buf[..n].to_vec(),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                t!(poll.poll(events, Some(Duration::from_millis(100))));
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
        assert!(Instant::now() < timeout, "data never delivered");
    }
}

/// A server registered for both readiness kinds, with nothing connected yet.
fn registered_server() -> (NamedPipe, String, Poll, Events) {
    let (mut server, name) = server();
    let poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));
    (server, name, poll, Events::with_capacity(16))
}

// Issue #1983: a pipe instance reused through `disconnect` and `connect` must
// not hand anything of the previous client to the next one. Here the previous
// client's EOF.
#[test]
fn reconnect_after_eof() {
    let (mut server, name, mut poll, mut events) = registered_server();
    for round in 0..2 {
        assert_would_block(server.connect());
        let msg = format!("round-{round}");
        let mut client = blocking_client(&name);
        t!(client.write_all(msg.as_bytes()));
        drop(client);

        assert_eq!(
            read_to_eof(&mut server, &mut poll, &mut events),
            msg.as_bytes()
        );
        t!(server.disconnect());
    }
}

// Same, with a read still in flight when the pipe disconnects.
#[test]
fn reconnect_with_read_in_flight() {
    let (mut server, name, mut poll, mut events) = registered_server();
    assert_would_block(server.connect());
    let mut first = blocking_client(&name);
    t!(first.write_all(b"first"));
    // Using up the data submits the next read, which stays in flight as the
    // client is still connected.
    assert_eq!(read_some(&mut server, &mut poll, &mut events), b"first");
    t!(server.disconnect());
    drop(first);

    assert_would_block(server.connect());
    let mut second = blocking_client(&name);
    t!(second.write_all(b"second"));
    drop(second);
    assert_eq!(read_to_eof(&mut server, &mut poll, &mut events), b"second");
}

// Same, with read data not handed out yet when the pipe disconnects.
#[test]
fn reconnect_with_unread_data() {
    let (mut server, name, mut poll, mut events) = registered_server();
    assert_would_block(server.connect());
    let mut first = blocking_client(&name);
    t!(first.write_all(b"first"));
    drop(first);
    // Readable means the data is read into the internal buffer, leave it there.
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::READABLE)],
    );
    t!(server.disconnect());

    assert_would_block(server.connect());
    let mut second = blocking_client(&name);
    t!(second.write_all(b"second"));
    drop(second);
    assert_eq!(read_to_eof(&mut server, &mut poll, &mut events), b"second");
}

// Same, with a write in flight when the client goes away, which then fails.
#[test]
fn reconnect_with_write_in_flight() {
    let (mut server, name, mut poll, mut events) = registered_server();
    assert_would_block(server.connect());
    let first = blocking_client(&name);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::WRITABLE)],
    );
    // Larger than the pipe's buffer, so the write pends as the client never
    // reads.
    let data = vec![0; 128 * 1024];
    assert_eq!(t!(server.write(&data)), data.len());
    drop(first);
    t!(server.disconnect());

    assert_would_block(server.connect());
    let _second = blocking_client(&name);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Interest::WRITABLE)],
    );
    assert_eq!(t!(server.write(b"second")), 6);
}

// Same, with a connect in flight when the pipe disconnects, which then fails.
#[test]
fn reconnect_after_aborted_connect() {
    let (server, name, mut poll, mut events) = registered_server();
    assert_would_block(server.connect());
    // Wine doesn't abort the connect, but fails here instead.
    let _ = server.disconnect();

    let timeout = Instant::now() + Duration::from_secs(10);
    let mut client = None;
    loop {
        t!(poll.poll(&mut events, Some(Duration::from_millis(100))));
        match server.connect() {
            Ok(()) => break,
            // A client can only open the pipe while a connect is in flight.
            Err(e) if e.kind() == io::ErrorKind::WouldBlock && client.is_none() => {
                client = Some(blocking_client(&name));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
        assert!(Instant::now() < timeout, "never connected");
    }
    match server.take_error() {
        Ok(None) => {}
        res => panic!("expected no error, got {res:?}"),
    }
}

// A reply written right before disconnecting, as servers typically do: its
// completion is handled after the disconnect.
#[test]
fn write_then_disconnect() {
    let (mut server, name, mut poll, mut events) = registered_server();
    for round in 0..2 {
        assert_would_block(server.connect());
        let _client = blocking_client(&name);
        expect_events(
            &mut poll,
            &mut events,
            vec![ExpectEvent::new(Token(0), Interest::WRITABLE)],
        );
        let reply = format!("reply-{round}");
        assert_eq!(t!(server.write(reply.as_bytes())), reply.len());
        t!(server.disconnect());
    }
    t!(poll.poll(&mut events, Some(Duration::from_millis(100))));
}

// Many clients in turn on one pipe: each only sees its own data, and nothing
// leaks.
#[test]
fn reconnect_many_clients() {
    let (mut server, name, mut poll, mut events) = registered_server();
    for round in 0..50 {
        assert_would_block(server.connect());
        let mut client = blocking_client(&name);
        let ping = format!("ping-{round}");
        t!(client.write_all(ping.as_bytes()));
        assert_eq!(
            read_some(&mut server, &mut poll, &mut events),
            ping.as_bytes()
        );

        let pong = format!("pong-{round}");
        assert_eq!(t!(server.write(pong.as_bytes())), pong.len());
        let mut buf = [0; 16];
        let n = t!(client.read(&mut buf));
        assert_eq!(&buf[..n], pong.as_bytes());

        drop(client);
        assert!(read_to_eof(&mut server, &mut poll, &mut events).is_empty());
        t!(server.disconnect());
    }
    drop(server);
    wait_freed(&name, || {
        t!(poll.poll(&mut events, Some(Duration::from_millis(100))));
    });
}

// Dropping a pipe doesn't cancel its write in flight, it's still delivered.
#[test]
fn drop_flushes_write_in_flight() {
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

    // Larger than the pipe's buffer, so the write pends until the client reads.
    let data = vec![1; 1024 * 1024];
    assert_eq!(t!(server.write(&data)), data.len());
    drop(server);

    let reader = thread::spawn(move || {
        let mut client = client;
        let mut read = Vec::new();
        t!(client.read_to_end(&mut read));
        read
    });
    // The write only finishes, and closes the pipe, while polling.
    let timeout = Instant::now() + Duration::from_secs(10);
    while !reader.is_finished() {
        t!(poll.poll(&mut events, Some(Duration::from_millis(100))));
        assert!(Instant::now() < timeout, "EOF never delivered");
    }
    let read = reader.join().unwrap();
    assert!(read == data, "got {} of {} bytes", read.len(), data.len());
}

// Issue #1944: dropping a pipe with a write in flight, and then its `Poll`,
// must still close the pipe. Nothing dequeues the write's completion anymore,
// so the pipe can't wait for it.
//
// Ignored until we agree on #1944. The plan: close the handle on drop, and
// make `flush` wait for the write in flight, so a write is only guaranteed
// once `flush` returns `Ok`.
#[test]
#[ignore = "fails until #1944 is fixed"]
fn drop_with_write_in_flight_then_poll_closes() {
    let (mut server, mut client) = pipe();
    let mut poll = t!(Poll::new());
    t!(poll.registry().register(
        &mut server,
        Token(0),
        Interest::READABLE | Interest::WRITABLE,
    ));
    t!(server.connect());

    // As if the client were another process, with its own event loop.
    let client_poll = t!(Poll::new());
    t!(client_poll.registry().register(
        &mut client,
        Token(1),
        Interest::READABLE | Interest::WRITABLE,
    ));
    // Larger than the pipe's buffer, so the write pends until the server reads.
    let data = vec![1; 1024 * 1024];
    assert_eq!(t!(client.write(&data)), data.len());
    drop(client);
    drop(client_poll);

    let mut events = Events::with_capacity(16);
    read_to_eof(&mut server, &mut poll, &mut events);
}
