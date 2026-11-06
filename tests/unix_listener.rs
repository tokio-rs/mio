#![cfg(all(unix, feature = "os-poll", feature = "net", not(miri)))] // Miri doesn't support Unix domain sockets.

use mio::net::UnixListener;
use mio::{Interest, Token};
use std::io::{self, Read};
use std::os::unix::net;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;

#[macro_use]
mod util;
use util::{
    assert_send, assert_socket_close_on_exec, assert_socket_non_blocking, assert_sync,
    assert_would_block, expect_events, expect_no_events, init_with_poll, temp_file, ExpectEvent,
};

const DEFAULT_BUF_SIZE: usize = 64;
const TOKEN_1: Token = Token(0);

#[test]
fn unix_listener_send_and_sync() {
    assert_send::<UnixListener>();
    assert_sync::<UnixListener>();
}

#[test]
fn unix_listener_smoke() {
    #[allow(clippy::redundant_closure)]
    smoke_test(|path| UnixListener::bind(path), "unix_listener_smoke");
}

#[test]
fn unix_listener_from_std() {
    smoke_test(
        |path| {
            let listener = net::UnixListener::bind(path).unwrap();
            // `std::os::unix::net::UnixStream`s are blocking by default, so make sure
            // it is in non-blocking mode before wrapping in a Mio equivalent.
            listener.set_nonblocking(true).unwrap();
            Ok(UnixListener::from_std(listener))
        },
        "unix_listener_from_std",
    )
}

#[test]
fn unix_listener_local_addr() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));

    let path = temp_file("unix_listener_local_addr");
    let mut listener = UnixListener::bind(&path).unwrap();
    poll.registry()
        .register(
            &mut listener,
            TOKEN_1,
            Interest::WRITABLE.add(Interest::READABLE),
        )
        .unwrap();

    let handle = open_connections(path.clone(), 1, barrier.clone());
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );

    let (stream, expected_addr) = listener.accept().unwrap();
    // getting pathname isn't supported on GNU/Hurd
    #[cfg(not(target_os = "hurd"))]
    assert_eq!(stream.local_addr().unwrap().as_pathname().unwrap(), &path);
    assert!(expected_addr.as_pathname().is_none());

    barrier.wait();
    handle.join().unwrap();
}

#[test]
#[cfg(any(target_os = "android", target_os = "linux"))]
fn unix_listener_accept_maximum_length_pathname() {
    use std::io::Write;
    use std::mem;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    fn sockaddr(path: &Path) -> (libc::sockaddr_un, usize) {
        // SAFETY: zero is valid for every field in `sockaddr_un`.
        let mut addr: libc::sockaddr_un = unsafe { mem::zeroed() };
        addr.sun_family = libc::AF_UNIX as _;
        let bytes = path.as_os_str().as_bytes();
        assert!(bytes.len() <= addr.sun_path.len());
        for (dst, src) in addr.sun_path.iter_mut().zip(bytes) {
            *dst = *src as _;
        }
        let offset = addr.sun_path.as_ptr() as usize - &addr as *const _ as usize;
        (addr, offset + bytes.len())
    }

    let (mut poll, mut events) = init_with_poll();
    let listener_path = temp_file("unix_listener_accept_maximum_length_pathname");
    let mut listener = UnixListener::bind(&listener_path).unwrap();
    poll.registry()
        .register(&mut listener, TOKEN_1, Interest::READABLE)
        .unwrap();

    let path = temp_file("unix_listener_accept_maximum_length_pathname_peer");
    let path = path.as_os_str().as_bytes();
    assert!(path.len() <= 108);
    let mut bytes = path.to_vec();
    bytes.resize(108, b'x');
    let peer_path = Path::new(std::ffi::OsStr::from_bytes(&bytes));
    let (addr, len) = sockaddr(peer_path);
    assert_eq!(peer_path.as_os_str().as_bytes().len(), addr.sun_path.len());

    // `SocketAddr::from_pathname` reserves space for a terminator, but Linux
    // also accepts a pathname occupying all 108 bytes of `sun_path`.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    assert!(fd >= 0, "{}", io::Error::last_os_error());
    // SAFETY: `socket` returned a valid, uniquely owned socket.
    let mut client = unsafe { net::UnixStream::from_raw_fd(fd) };
    let result = unsafe {
        libc::bind(
            client.as_raw_fd(),
            &addr as *const _ as *const libc::sockaddr,
            len as libc::socklen_t,
        )
    };
    assert_eq!(result, 0, "{}", io::Error::last_os_error());

    let (addr, len) = sockaddr(&listener_path);
    let result = unsafe {
        libc::connect(
            client.as_raw_fd(),
            &addr as *const _ as *const libc::sockaddr,
            (len + 1) as libc::socklen_t,
        )
    };
    assert_eq!(result, 0, "{}", io::Error::last_os_error());
    client.write_all(b"hello").unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );

    let (mut accepted, address) = listener.accept().unwrap();
    assert_eq!(address.as_pathname(), Some(peer_path));
    assert_socket_non_blocking(&accepted);
    assert_socket_close_on_exec(&accepted);
    let mut buf = [0; 5];
    accepted.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"hello");
}

#[test]
fn unix_listener_register() {
    let (mut poll, mut events) = init_with_poll();

    let path = temp_file("unix_listener_register");
    let mut listener = UnixListener::bind(path).unwrap();
    poll.registry()
        .register(&mut listener, TOKEN_1, Interest::READABLE)
        .unwrap();
    expect_no_events(&mut poll, &mut events)
}

#[test]
fn unix_listener_reregister() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));

    let path = temp_file("unix_listener_reregister");
    let mut listener = UnixListener::bind(&path).unwrap();
    poll.registry()
        .register(&mut listener, TOKEN_1, Interest::WRITABLE)
        .unwrap();

    let handle = open_connections(path, 1, barrier.clone());
    expect_no_events(&mut poll, &mut events);

    poll.registry()
        .reregister(&mut listener, TOKEN_1, Interest::READABLE)
        .unwrap();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );
    // Complete handshake to unblock the client thread.
    #[cfg(target_os = "cygwin")]
    listener.accept().unwrap();

    barrier.wait();
    handle.join().unwrap();
}

#[test]
fn unix_listener_reregisters_after_would_block() {
    let (mut poll, mut events) = init_with_poll();
    let path = temp_file("unix_listener_reregisters_after_would_block");
    let mut listener = UnixListener::bind(&path).unwrap();
    poll.registry()
        .register(&mut listener, TOKEN_1, Interest::READABLE)
        .unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let handle = thread::spawn({
        let barrier = barrier.clone();
        move || {
            let first = net::UnixStream::connect(&path).unwrap();
            barrier.wait();
            barrier.wait();
            let second = net::UnixStream::connect(&path).unwrap();
            drop((first, second));
        }
    });

    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );
    listener.accept().unwrap();
    barrier.wait();
    assert_would_block(listener.accept());

    barrier.wait();
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );
    listener.accept().unwrap();
    handle.join().unwrap();
}

#[test]
fn unix_listener_deregister() {
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));

    let path = temp_file("unix_listener_deregister");
    let mut listener = UnixListener::bind(&path).unwrap();
    poll.registry()
        .register(&mut listener, TOKEN_1, Interest::READABLE)
        .unwrap();

    let handle = open_connections(path, 1, barrier.clone());

    poll.registry().deregister(&mut listener).unwrap();
    expect_no_events(&mut poll, &mut events);
    // Complete handshake to unblock the client thread.
    #[cfg(target_os = "cygwin")]
    listener.accept().unwrap();

    barrier.wait();
    handle.join().unwrap();
}

#[test]
#[cfg(any(target_os = "android", target_os = "linux"))]
fn unix_listener_abstract_namespace() {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::SocketAddr;

    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));

    let num: u64 = rand::random();
    let name = format!("mio-abstract-uds-{num}");
    let address = SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
    let mut listener = UnixListener::bind_addr(&address).unwrap();
    assert_eq!(
        listener.local_addr().unwrap().as_abstract_name(),
        address.as_abstract_name(),
    );

    poll.registry()
        .register(
            &mut listener,
            TOKEN_1,
            Interest::WRITABLE.add(Interest::READABLE),
        )
        .unwrap();
    expect_no_events(&mut poll, &mut events);

    let barrier2 = barrier.clone();
    let handle = thread::spawn(move || {
        let conn = net::UnixStream::connect_addr(&address).unwrap();
        barrier2.wait();
        drop(conn);
    });
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );

    let (_, address) = listener.accept().unwrap();
    assert!(address.is_unnamed());

    barrier.wait();
    handle.join().unwrap();
}

fn smoke_test<F>(new_listener: F, test_name: &'static str)
where
    F: FnOnce(&Path) -> io::Result<UnixListener>,
{
    let (mut poll, mut events) = init_with_poll();
    let barrier = Arc::new(Barrier::new(2));
    let path = temp_file(test_name);

    let mut listener = new_listener(&path).unwrap();

    assert_socket_non_blocking(&listener);
    assert_socket_close_on_exec(&listener);

    poll.registry()
        .register(
            &mut listener,
            TOKEN_1,
            Interest::WRITABLE.add(Interest::READABLE),
        )
        .unwrap();
    expect_no_events(&mut poll, &mut events);

    let handle = open_connections(path, 1, barrier.clone());
    expect_events(
        &mut poll,
        &mut events,
        vec![ExpectEvent::new(TOKEN_1, Interest::READABLE)],
    );

    let (mut stream, _) = listener.accept().unwrap();

    let mut buf = [0; DEFAULT_BUF_SIZE];
    assert_would_block(stream.read(&mut buf));

    assert_would_block(listener.accept());
    assert!(listener.take_error().unwrap().is_none());

    barrier.wait();
    handle.join().unwrap();
}

fn open_connections(
    path: PathBuf,
    n_connections: usize,
    barrier: Arc<Barrier>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for _ in 0..n_connections {
            let conn = net::UnixStream::connect(path.clone()).unwrap();
            barrier.wait();
            drop(conn);
        }
    })
}
