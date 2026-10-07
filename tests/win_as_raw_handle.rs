#![cfg(all(windows, feature = "os-poll", feature = "net"))]

use std::net::TcpStream;
use std::os::windows::io::{AsRawHandle, RawHandle};
use std::thread;
use std::time::{Duration, Instant};

use mio::net::TcpListener;
use mio::{Interest, Token, Waker};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

mod util;
use util::{any_local_address, expect_events, init_with_poll, ExpectEvent, Readiness};

const ZERO: Option<Duration> = Some(Duration::ZERO);

fn wait(handle: RawHandle, timeout_ms: u32) -> u32 {
    unsafe { WaitForSingleObject(handle, timeout_ms) }
}

#[test]
fn handle_is_signaled_by_waker() {
    let (mut poll, mut events) = init_with_poll();
    let handle = poll.as_raw_handle();
    assert_eq!(handle, poll.registry().as_raw_handle());

    assert_eq!(wait(handle, 0), WAIT_TIMEOUT);

    let waker = Waker::new(poll.registry(), Token(10)).unwrap();
    waker.wake().unwrap();

    // Waiting does not consume the completion packet.
    assert_eq!(wait(handle, 0), WAIT_OBJECT_0);
    assert_eq!(wait(handle, 0), WAIT_OBJECT_0);

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(10), Readiness::READABLE)],
    );

    assert_eq!(wait(handle, 0), WAIT_TIMEOUT);
}

#[test]
fn handle_is_signaled_by_socket_readiness() {
    let (mut poll, mut events) = init_with_poll();
    let handle = poll.as_raw_handle();

    let mut listener = TcpListener::bind(any_local_address()).unwrap();
    let addr = listener.local_addr().unwrap();
    poll.registry()
        .register(&mut listener, Token(1), Interest::READABLE)
        .unwrap();

    // Arm the readiness poll on the socket.
    poll.poll(&mut events, ZERO).unwrap();
    assert!(events.is_empty());
    assert_eq!(wait(handle, 0), WAIT_TIMEOUT);

    let connect = thread::spawn(move || TcpStream::connect(addr).unwrap());

    let start = Instant::now();
    while wait(handle, 50) != WAIT_OBJECT_0 {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "handle never signaled"
        );
    }

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(Token(1), Readiness::READABLE)],
    );

    let _stream = connect.join().unwrap();
}
