// Miri doesn't support waiting on an epoll fd.
#![cfg(all(feature = "os-poll", feature = "net", any(unix, windows), not(miri)))]

use std::io::{self, Read, Write};
use std::net;
use std::thread;
use std::time::Duration;

use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token, Waker};

mod util;
use util::{any_local_address, expect_events, init_with_poll, ExpectEvent, Readiness};

const SUPPORTED: bool = cfg!(any(
    windows,
    all(
        not(mio_unsupported_force_poll_poll),
        any(
            target_os = "android",
            target_os = "dragonfly",
            target_os = "emscripten",
            target_os = "freebsd",
            target_os = "illumos",
            target_os = "ios",
            target_os = "linux",
            target_os = "macos",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "redox",
            target_os = "tvos",
            target_os = "visionos",
            target_os = "watchos",
        ),
    ),
));

const TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(unix)]
type Handle = std::os::fd::RawFd;
#[cfg(windows)]
type Handle = std::os::windows::io::RawHandle;

fn init() -> Option<(Poll, Events)> {
    SUPPORTED.then(init_with_poll)
}

#[cfg(unix)]
fn prepare(poll: &Poll) -> Handle {
    use std::os::fd::AsRawFd;
    poll.prepare_external_wait().unwrap().as_raw_fd()
}

#[cfg(windows)]
fn prepare(poll: &Poll) -> Handle {
    use std::os::windows::io::AsRawHandle;
    poll.prepare_external_wait().unwrap().as_raw_handle()
}

#[cfg(unix)]
fn is_ready(handle: Handle, timeout: Duration) -> bool {
    let mut pollfd = libc::pollfd {
        fd: handle,
        events: libc::POLLIN,
        revents: 0,
    };
    let n = unsafe { libc::poll(&mut pollfd, 1, timeout.as_millis() as libc::c_int) };
    assert!(n >= 0, "poll failed: {}", io::Error::last_os_error());
    n == 1
}

#[cfg(windows)]
fn is_ready(handle: Handle, timeout: Duration) -> bool {
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    match unsafe { WaitForSingleObject(handle, timeout.as_millis() as u32) } {
        WAIT_OBJECT_0 => true,
        WAIT_TIMEOUT => false,
        res => panic!(
            "unexpected wait result {res:#x}: {}",
            io::Error::last_os_error()
        ),
    }
}

/// Drives `poll` as a host event loop would until a matching event for
/// `token` is received, requiring every wait to end with the handle ready.
#[track_caller]
fn host_wait_for(poll: &mut Poll, events: &mut Events, token: Token, f: fn(&Event) -> bool) {
    for _ in 0..10 {
        assert!(
            is_ready(prepare(poll), TIMEOUT),
            "handle not ready while waiting for {token:?}"
        );
        poll.poll(events, Some(Duration::ZERO)).unwrap();
        if events
            .iter()
            .any(|event| event.token() == token && f(event))
        {
            return;
        }
    }
    panic!("no event for {token:?}");
}

/// Returns a connected (non-blocking mio, blocking std) pair of streams.
fn stream_pair() -> (TcpStream, net::TcpStream) {
    let listener = net::TcpListener::bind(any_local_address()).unwrap();
    let peer = net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    // Emscripten does not support blocking accept.
    listener.set_nonblocking(true).unwrap();
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => thread::yield_now(),
            Err(err) => panic!("unexpected accept error: {err}"),
        }
    };
    stream.set_nonblocking(true).unwrap();
    (TcpStream::from_std(stream), peer)
}

fn read_until_would_block(stream: &mut TcpStream) {
    let mut buf = [0; 64];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => panic!("unexpected end of stream"),
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => return,
            Err(err) => panic!("unexpected read error: {err}"),
        }
    }
}

#[test]
fn support() {
    let (poll, _) = init_with_poll();
    let expected = if SUPPORTED {
        Ok(())
    } else {
        Err(io::ErrorKind::Unsupported)
    };
    let result = poll.prepare_external_wait().map(drop);
    assert_eq!(result.map_err(|err| err.kind()), expected);
}

#[test]
fn not_ready_without_events() {
    let Some((poll, _)) = init() else { return };
    assert!(!is_ready(prepare(&poll), Duration::ZERO));
}

#[test]
fn waiting_does_not_consume_events() {
    let Some((mut poll, mut events)) = init() else {
        return;
    };
    let waker = Waker::new(poll.registry(), Token(0)).unwrap();

    waker.wake().unwrap();
    let handle = prepare(&poll);
    assert!(is_ready(handle, Duration::ZERO));
    assert!(is_ready(handle, Duration::ZERO));

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
    );
    assert!(!is_ready(prepare(&poll), Duration::ZERO));
}

#[test]
fn blocked_wait_is_woken_without_consuming_events() {
    let Some((mut poll, mut events)) = init() else {
        return;
    };
    let waker = Waker::new(poll.registry(), Token(0)).unwrap();

    poll.poll(&mut events, Some(Duration::ZERO)).unwrap();
    let handle = prepare(&poll);

    let wake = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        waker.wake().unwrap();
        waker
    });
    assert!(is_ready(handle, TIMEOUT));
    let _waker = wake.join().unwrap();

    assert!(is_ready(handle, Duration::ZERO));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
    );
    assert!(!is_ready(prepare(&poll), Duration::ZERO));
}

#[test]
fn blocked_wait_on_other_thread_is_woken_without_consuming_events() {
    let Some((mut poll, mut events)) = init() else {
        return;
    };
    let waker = Waker::new(poll.registry(), Token(0)).unwrap();

    // `RawHandle` isn't `Send`.
    let handle = prepare(&poll) as usize;
    let waiter = thread::spawn(move || is_ready(handle as Handle, TIMEOUT));
    thread::sleep(Duration::from_millis(100));
    waker.wake().unwrap();
    assert!(waiter.join().unwrap());

    assert!(is_ready(handle as Handle, Duration::ZERO));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
    );
    assert!(!is_ready(prepare(&poll), Duration::ZERO));
}

#[test]
fn registration_before_prepare() {
    let Some((mut poll, mut events)) = init() else {
        return;
    };
    let mut listener = TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();

    // Without a `Poll::poll` since the registration.
    let handle = prepare(&poll);
    assert!(!is_ready(handle, Duration::ZERO));

    let _peer = net::TcpStream::connect(addr).unwrap();
    assert!(is_ready(handle, TIMEOUT));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
    );
}

#[test]
fn would_block_before_prepare() {
    let Some((mut poll, mut events)) = init() else {
        return;
    };
    let (mut stream, mut peer) = stream_pair();
    poll.registry()
        .register(&mut stream, Token(0), Interest::READABLE)
        .unwrap();

    peer.write_all(b"a").unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
    );
    // Without a `Poll::poll` since the `WouldBlock`.
    read_until_would_block(&mut stream);

    let handle = prepare(&poll);
    assert!(!is_ready(handle, Duration::ZERO));

    peer.write_all(b"b").unwrap();
    assert!(is_ready(handle, TIMEOUT));
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
    );
}

#[test]
fn would_block_on_other_thread_after_prepare() {
    let Some((mut poll, mut events)) = init() else {
        return;
    };
    let (mut stream, mut peer) = stream_pair();
    poll.registry()
        .register(&mut stream, Token(0), Interest::READABLE)
        .unwrap();

    peer.write_all(b"a").unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
    );

    // The event was received, so the unread data no longer makes the handle
    // ready.
    let handle = prepare(&poll);
    assert!(!is_ready(handle, Duration::ZERO));

    let sockets = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        read_until_would_block(&mut stream);
        peer.write_all(b"b").unwrap();
        (stream, peer)
    });
    // On Windows the `WouldBlock` cancels the pending poll, which only
    // watched for errors after the event, and the cancellation readies the
    // handle without an event.
    assert!(is_ready(handle, TIMEOUT));
    let _sockets = sockets.join().unwrap();

    host_wait_for(&mut poll, &mut events, Token(0), Event::is_readable);
}

#[test]
fn registration_on_other_thread_after_prepare() {
    let Some((mut poll, mut events)) = init() else {
        return;
    };
    let registry = poll.registry().try_clone().unwrap();

    let handle = prepare(&poll);
    let sockets = thread::spawn(move || {
        let mut listener = TcpListener::bind(any_local_address()).unwrap();
        registry
            .register(&mut listener, Token(0), Interest::READABLE)
            .unwrap();
        let peer = net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        (listener, peer)
    });
    assert!(is_ready(handle, TIMEOUT));
    let _sockets = sockets.join().unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
    );
}

#[test]
fn reregistration_on_other_thread_after_prepare() {
    let Some((mut poll, mut events)) = init() else {
        return;
    };
    let (mut stream, _peer) = stream_pair();
    poll.registry()
        .register(&mut stream, Token(0), Interest::READABLE)
        .unwrap();
    let registry = poll.registry().try_clone().unwrap();

    let handle = prepare(&poll);
    assert!(!is_ready(handle, Duration::ZERO));

    let stream = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        registry
            .reregister(&mut stream, Token(1), Interest::WRITABLE)
            .unwrap();
        stream
    });
    // On Windows the change cancels the pending readable poll, and the
    // cancellation readies the handle without an event.
    assert!(is_ready(handle, TIMEOUT));
    let _stream = stream.join().unwrap();

    host_wait_for(&mut poll, &mut events, Token(1), Event::is_writable);
}

#[test]
#[cfg(windows)]
fn ready_without_events_after_deregistration() {
    let (mut poll, mut events) = init_with_poll();
    let mut listener = TcpListener::bind(any_local_address()).unwrap();
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();
    let handle = prepare(&poll);
    assert!(!is_ready(handle, Duration::ZERO));

    poll.registry().deregister(&mut listener).unwrap();
    assert!(is_ready(handle, TIMEOUT));

    poll.poll(&mut events, Some(Duration::ZERO)).unwrap();
    assert!(events.is_empty());
    assert!(!is_ready(prepare(&poll), Duration::ZERO));
}

#[test]
#[cfg(windows)]
fn wait_for_multiple_objects() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForMultipleObjects};
    use windows_sys::Win32::UI::WindowsAndMessaging::{MsgWaitForMultipleObjectsEx, QS_ALLINPUT};

    let (mut poll, mut events) = init_with_poll();
    let waker = Waker::new(poll.registry(), Token(0)).unwrap();
    let event = unsafe { CreateEventW(null(), 1, 0, null()) };
    assert!(!event.is_null(), "{}", io::Error::last_os_error());
    let event = unsafe { OwnedHandle::from_raw_handle(event) };

    let handles = [event.as_raw_handle(), prepare(&poll)];
    let wait_any = || unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, 0) };
    let msg_wait_any =
        || unsafe { MsgWaitForMultipleObjectsEx(2, handles.as_ptr(), 0, QS_ALLINPUT, 0) };
    assert_eq!(wait_any(), WAIT_TIMEOUT);
    assert_eq!(msg_wait_any(), WAIT_TIMEOUT);

    let wake = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        waker.wake().unwrap();
        waker
    });
    let res = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, TIMEOUT.as_millis() as u32) };
    assert_eq!(res, WAIT_OBJECT_0 + 1);
    let _waker = wake.join().unwrap();

    assert_eq!(wait_any(), WAIT_OBJECT_0 + 1);
    assert_eq!(msg_wait_any(), WAIT_OBJECT_0 + 1);
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
    );
}
