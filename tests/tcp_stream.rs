use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::{self, Shutdown, SocketAddr};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::sync::mpsc::channel;
use std::sync::{Arc, Barrier};
use std::thread;

use mio::net::TcpStream;
use mio::{Interests, Token};

mod util;

use util::{
    any_local_address, any_local_ipv6_address, assert_send, assert_sync, assert_would_block,
    expect_events, expect_no_events, init, init_with_poll, ExpectEvent,
};

const DATA1: &[u8] = b"Hello world!";
const DATA2: &[u8] = b"Hello mars!";
// TODO: replace with `DATA1.len()` once `const_slice_len` is stable.
const DATA1_LEN: usize = 12;
const DATA2_LEN: usize = 11;

const ID1: Token = Token(0);
const ID2: Token = Token(1);

#[test]
fn is_send_and_sync() {
    assert_send::<TcpStream>();
    assert_sync::<TcpStream>();
}

#[test]
#[cfg_attr(windows, ignore = "fails on Windows, see #1078")]
fn tcp_stream_ipv4() {
    smoke_test_tcp_stream(any_local_address());
}

#[test]
#[cfg_attr(windows, ignore = "fails on Windows, see #1078")]
fn tcp_stream_ipv6() {
    smoke_test_tcp_stream(any_local_ipv6_address());
}

fn smoke_test_tcp_stream(addr: SocketAddr) {
    let (mut poll, mut events) = init_with_poll();

    let (thread_handle, addr) = echo_listener(addr, 1);
    let mut stream = TcpStream::connect(addr).unwrap();

    poll.registry()
        .register(&stream, ID1, Interests::WRITABLE.add(Interests::READABLE))
        .expect("unable to register TCP stream");

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::WRITABLE)],
    );

    let mut buf = [0; 16];
    assert_would_block(stream.peek(&mut buf));
    assert_would_block(stream.read(&mut buf));

    // NOTE: the call to `peer_addr` must happen after we received a writable
    // event as the stream might not yet be connected.
    assert_eq!(stream.peer_addr().unwrap(), addr);
    assert!(stream.local_addr().unwrap().ip().is_loopback());

    let n = stream.write(&DATA1).expect("unable to write to stream");
    assert_eq!(n, DATA1.len());

    stream.flush().unwrap();

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );

    let n = stream.peek(&mut buf).expect("unable to peek from stream");
    assert_eq!(n, DATA1.len());
    assert_eq!(&buf[..n], DATA1);

    let n = stream.read(&mut buf).expect("unable to read from stream");
    assert_eq!(n, DATA1.len());
    assert_eq!(&buf[..n], DATA1);

    assert!(stream.take_error().unwrap().is_none());

    let bufs = [IoSlice::new(&DATA1), IoSlice::new(&DATA2)];
    let n = stream
        .write_vectored(&bufs)
        .expect("unable to write vectored to stream");
    assert_eq!(n, DATA1.len() + DATA2.len());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );

    let mut buf1 = [1; DATA1_LEN];
    let mut buf2 = [2; DATA2_LEN + 1];
    let mut bufs = [IoSliceMut::new(&mut buf1), IoSliceMut::new(&mut buf2)];
    let n = stream
        .read_vectored(&mut bufs)
        .expect("unable to read vectored from stream");
    assert_eq!(n, DATA1.len() + DATA2.len());
    assert_eq!(&buf1, DATA1);
    assert_eq!(&buf2[..DATA2.len()], DATA2);
    assert_eq!(buf2[DATA2.len()], 2); // Last byte should be unchanged.

    // Close the connection to allow the listener to shutdown.
    drop(stream);

    thread_handle.join().expect("unable to join thread");
}

#[test]
fn try_clone() {
    let (mut poll, mut events) = init_with_poll();

    let (thread_handle, address) = echo_listener(any_local_address(), 1);

    let mut stream1 = TcpStream::connect(address).unwrap();

    poll.registry()
        .register(&stream1, ID1, Interests::WRITABLE)
        .expect("unable to register TCP stream");

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::WRITABLE)],
    );

    let n = stream1.write(DATA1).unwrap();
    assert_eq!(n, DATA1.len());

    let mut stream2 = stream1.try_clone().unwrap();

    // When using `try_clone` the `TcpStream` needs to be deregistered!
    poll.registry().deregister(&stream1).unwrap();
    drop(stream1);

    poll.registry()
        .register(&stream2, ID2, Interests::READABLE)
        .expect("unable to register TCP stream");

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID2, Interests::READABLE)],
    );

    let mut buf = [0; 20];
    let n = stream2.read(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(&buf[..n], DATA1);

    drop(stream2);
    thread_handle.join().expect("unable to join thread");
}

#[test]
fn ttl() {
    init();

    let barrier = Arc::new(Barrier::new(2));
    let (thread_handle, address) = start_listener(1, Some(barrier.clone()));

    let stream = TcpStream::connect(address).unwrap();

    const TTL: u32 = 10;
    stream.set_ttl(TTL).unwrap();
    assert_eq!(stream.ttl().unwrap(), TTL);
    assert!(stream.take_error().unwrap().is_none());

    barrier.wait();
    thread_handle.join().expect("unable to join thread");
}

#[test]
fn nodelay() {
    let (mut poll, mut events) = init_with_poll();

    let barrier = Arc::new(Barrier::new(2));
    let (thread_handle, address) = start_listener(1, Some(barrier.clone()));

    let stream = TcpStream::connect(address).unwrap();

    poll.registry()
        .register(&stream, ID1, Interests::WRITABLE)
        .expect("unable to register TCP stream");

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::WRITABLE)],
    );

    const NO_DELAY: bool = true;
    stream.set_nodelay(NO_DELAY).unwrap();
    assert_eq!(stream.nodelay().unwrap(), NO_DELAY);
    assert!(stream.take_error().unwrap().is_none());

    barrier.wait();
    thread_handle.join().expect("unable to join thread");
}

#[test]
fn shutdown_read() {
    let (mut poll, mut events) = init_with_poll();

    let (thread_handle, address) = echo_listener(any_local_address(), 1);

    let mut stream = TcpStream::connect(address).unwrap();

    poll.registry()
        .register(&stream, ID1, Interests::WRITABLE.add(Interests::READABLE))
        .expect("unable to register TCP stream");

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::WRITABLE)],
    );

    let n = stream.write(DATA2).unwrap();
    assert_eq!(n, DATA2.len());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );

    stream.shutdown(Shutdown::Read).unwrap();

    // Shutting down the reading side is different on each platform. For example
    // on Linux based systems we can still read.
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        let mut buf = [0; 20];
        let n = stream.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }

    drop(stream);
    thread_handle.join().expect("unable to join thread");
}

#[test]
#[ignore = "This test is flaky, it doesn't always receive an event after shutting down the write side"]
fn shutdown_write() {
    let (mut poll, mut events) = init_with_poll();

    let (thread_handle, address) = echo_listener(any_local_address(), 1);

    let mut stream = TcpStream::connect(address).unwrap();

    poll.registry()
        .register(&stream, ID1, Interests::WRITABLE.add(Interests::READABLE))
        .expect("unable to register TCP stream");

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::WRITABLE)],
    );

    let n = stream.write(DATA1).unwrap();
    assert_eq!(n, DATA1.len());

    stream.shutdown(Shutdown::Write).unwrap();

    let err = stream.write(DATA2).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);

    // FIXME: we don't always receive the following event.
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );

    // Read should be ok.
    let mut buf = [0; 20];
    let n = stream.read(&mut buf).unwrap();
    assert_eq!(n, DATA1.len());
    assert_eq!(&buf[..n], DATA1);

    drop(stream);
    thread_handle.join().expect("unable to join thread");
}

#[test]
fn shutdown_both() {
    let (mut poll, mut events) = init_with_poll();

    let (thread_handle, address) = echo_listener(any_local_address(), 1);

    let mut stream = TcpStream::connect(address).unwrap();

    poll.registry()
        .register(&stream, ID1, Interests::WRITABLE.add(Interests::READABLE))
        .expect("unable to register TCP stream");

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::WRITABLE)],
    );

    let n = stream.write(DATA1).unwrap();
    assert_eq!(n, DATA1.len());

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID1, Interests::READABLE)],
    );

    stream.shutdown(Shutdown::Both).unwrap();

    // Shutting down the reading side is different on each platform. For example
    // on Linux based systems we can still read.
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        let mut buf = [0; 20];
        let n = stream.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }

    let err = stream.write(DATA2).unwrap_err();
    #[cfg(unix)]
    assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    #[cfg(windows)]
    assert_eq!(err.kind(), io::ErrorKind::ConnectionAborted);

    drop(stream);
    thread_handle.join().expect("unable to join thread");
}

#[test]
#[cfg(unix)]
fn raw_fd() {
    init();

    let (thread_handle, address) = start_listener(1, None);

    let stream = TcpStream::connect(address).unwrap();
    let address = stream.local_addr().unwrap();

    let raw_fd1 = stream.as_raw_fd();
    let raw_fd2 = stream.into_raw_fd();
    assert_eq!(raw_fd1, raw_fd2);

    let stream = unsafe { TcpStream::from_raw_fd(raw_fd2) };
    assert_eq!(stream.as_raw_fd(), raw_fd1);
    assert_eq!(stream.local_addr().unwrap(), address);

    thread_handle.join().expect("unable to join thread");
}

#[test]
fn registering() {
    let (mut poll, mut events) = init_with_poll();

    let (thread_handle, address) = echo_listener(any_local_address(), 1);

    let stream = TcpStream::connect(address).unwrap();

    poll.registry()
        .register(&stream, ID1, Interests::READABLE)
        .expect("unable to register TCP stream");

    expect_no_events(&mut poll, &mut events);

    // NOTE: more tests are done in the smoke tests above.

    drop(stream);
    thread_handle.join().expect("unable to join thread");
}

#[test]
fn reregistering() {
    let (mut poll, mut events) = init_with_poll();

    let (thread_handle, address) = echo_listener(any_local_address(), 1);

    let stream = TcpStream::connect(address).unwrap();

    poll.registry()
        .register(&stream, ID1, Interests::READABLE)
        .expect("unable to register TCP stream");

    poll.registry()
        .reregister(&stream, ID2, Interests::WRITABLE)
        .expect("unable to reregister TCP stream");

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(ID2, Interests::WRITABLE)],
    );

    assert_eq!(stream.peer_addr().unwrap(), address);

    drop(stream);
    thread_handle.join().expect("unable to join thread");
}

#[test]
fn deregistering() {
    let (mut poll, mut events) = init_with_poll();

    let (thread_handle, address) = echo_listener(any_local_address(), 1);

    let stream = TcpStream::connect(address).unwrap();

    poll.registry()
        .register(&stream, ID1, Interests::READABLE)
        .expect("unable to register TCP stream");

    poll.registry()
        .deregister(&stream)
        .expect("unable to deregister TCP stream");

    expect_no_events(&mut poll, &mut events);

    // We do expect to be connected.
    assert_eq!(stream.peer_addr().unwrap(), address);

    drop(stream);
    thread_handle.join().expect("unable to join thread");
}

/// Start a listener that accepts `n_connections` connections on the returned
/// address. It echos back any data it reads from the connection before
/// accepting another one.
fn echo_listener(addr: SocketAddr, n_connections: usize) -> (thread::JoinHandle<()>, SocketAddr) {
    let (sender, receiver) = channel();
    let thread_handle = thread::spawn(move || {
        let listener = net::TcpListener::bind(addr).unwrap();
        let local_address = listener.local_addr().unwrap();
        sender.send(local_address).unwrap();

        let mut buf = [0; 128];
        for _ in 0..n_connections {
            let (mut stream, _) = listener.accept().unwrap();

            loop {
                let n = stream
                    .read(&mut buf)
                    // On Linux based system it will cause a connection reset
                    // error when the reading side of the peer connection is
                    // shutdown, we don't consider it an actual here.
                    .or_else(|err| match err {
                        ref err if err.kind() == io::ErrorKind::ConnectionReset => Ok(0),
                        err => Err(err),
                    })
                    .expect("error reading");
                if n == 0 {
                    break;
                }
                let written = stream.write(&buf[..n]).expect("error writing");
                assert_eq!(written, n, "short write");
            }
        }
    });
    (thread_handle, receiver.recv().unwrap())
}

/// Start a listener that accepts `n_connections` connections on the returned
/// address. If a barrier is provided it will wait on it before closing the
/// connection.
fn start_listener(
    n_connections: usize,
    barrier: Option<Arc<Barrier>>,
) -> (thread::JoinHandle<()>, SocketAddr) {
    let (sender, receiver) = channel();
    let thread_handle = thread::spawn(move || {
        let listener = net::TcpListener::bind(any_local_address()).unwrap();
        let local_address = listener.local_addr().unwrap();
        sender.send(local_address).unwrap();

        for _ in 0..n_connections {
            let (stream, _) = listener.accept().unwrap();
            if let Some(ref barrier) = barrier {
                barrier.wait();
            }
            drop(stream);
        }
    });
    (thread_handle, receiver.recv().unwrap())
}
