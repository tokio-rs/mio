#[macro_use]
mod util;

use mio::event::{Events, Source};
use mio::net::{TcpListener, TcpStream};
use mio::{Interests, Poll, Token};
use std::io::{Read, Result, Write};
use std::net::{Shutdown, SocketAddr};
use std::sync::{mpsc, Arc, Barrier};
use std::thread::{self, JoinHandle};
use util::{any_local_address, any_local_ipv6_address, expect_events, init_with_poll, ExpectEvent};

const DATA: &[u8] = b"hello";
const TOK: Token = Token(0);

macro_rules! generate_tests {
    ($any_addr:expr) => {
        #[test]
        fn echo_server() {
            let addr = $any_addr();
            let barrier = Arc::new(Barrier::new(2));
            let (mut poll, mut events) = init_with_poll();
            let (handle, addr) = start_echo_server(addr, barrier.clone());

            let stream = TcpStream::connect(addr).expect("failed to connect to listener");
            barrier.wait();

            assert_ok!(register(
                &mut poll,
                &mut events,
                &stream,
                Interests::WRITABLE,
                || Ok(()),
                || (&stream).write(DATA),
            ));

            // Wait for peer to read and write data
            barrier.wait();

            assert_ok!(register(
                &mut poll,
                &mut events,
                &stream,
                Interests::READABLE,
                || Ok(()),
                || Ok(()),
            ));

            assert_ok!(register(
                &mut poll,
                &mut events,
                &stream,
                Interests::READ_CLOSE,
                || stream.shutdown(Shutdown::Read),
                || Ok(()),
            ));

            assert_ok!(register(
                &mut poll,
                &mut events,
                &stream,
                Interests::WRITE_CLOSE,
                || stream.shutdown(Shutdown::Write),
                || Ok(()),
            ));

            // Unblock peer and close connection
            barrier.wait();
            drop(stream);

            assert_ok!(handle.join())
        }

        #[test]
        fn client_shutdown() {
            let addr = $any_addr();
            let barrier = Arc::new(Barrier::new(2));
            let (mut poll, mut events) = init_with_poll();
            let (handle, addr) = start_noop_server(addr, barrier.clone());

            let stream = TcpStream::connect(addr).expect("failed to connect to listener");
            barrier.wait();

            assert_ok!(register(
                &mut poll,
                &mut events,
                &stream,
                Interests::WRITABLE,
                || Ok(()),
                || Ok(()),
            ));

            assert_ok!(register(
                &mut poll,
                &mut events,
                &stream,
                Interests::READ_CLOSE | Interests::WRITE_CLOSE,
                || stream.shutdown(Shutdown::Both),
                || Ok(()),
            ));

            // Unblock peer and close connection
            barrier.wait();
            drop(stream);

            assert_ok!(handle.join())
        }

        #[test]
        fn server_write_shutdown() {
            let addr = $any_addr();
            let barrier = Arc::new(Barrier::new(2));
            let (mut poll, mut events) = init_with_poll();
            let (handle, addr) = start_write_shutdown_server(addr, barrier.clone());

            let stream = TcpStream::connect(addr).expect("failed to connect to listener");
            barrier.wait();

            assert_ok!(register(
                &mut poll,
                &mut events,
                &stream,
                Interests::READ_CLOSE,
                || Ok(()),
                || Ok(()),
            ));

            assert_ok!(register(
                &mut poll,
                &mut events,
                &stream,
                Interests::WRITABLE,
                || Ok(()),
                || Ok(()),
            ));

            // Unblock peer and close connection
            barrier.wait();
            drop(stream);

            assert_ok!(handle.join())
        }
    };
}

mod ipv4 {
    use super::*;
    generate_tests!(any_local_address);
}

mod ipv6 {
    use super::*;
    generate_tests!(any_local_ipv6_address);
}

fn start_echo_server(address: SocketAddr, barrier: Arc<Barrier>) -> (JoinHandle<()>, SocketAddr) {
    let (send, recv) = mpsc::channel();
    let handle = thread::spawn(move || {
        let listener = TcpListener::bind(address).expect("failed to bind address");
        let local_address = listener.local_addr().expect("failed to get local address");
        send.send(local_address)
            .expect("failed to send local address");

        // Unblock the first peer and accept connection
        barrier.wait();
        let (mut stream, _) = listener
            .accept()
            .expect("failed to accept first connection");

        // Wait for data on the stream
        barrier.wait();

        // Read data and immediately write it back
        let mut buf = [0; 20];
        let read = stream.read(&mut buf).expect("failed to read from stream");
        let written = stream
            .write(&buf[..read])
            .expect("failed to write to stream");
        assert_eq!(read, written);

        // Wait for the first peer to close connection and drop the stream
        barrier.wait();
        drop(stream);
    });
    (
        handle,
        recv.recv().expect("failed to receive local address"),
    )
}

fn start_noop_server(address: SocketAddr, barrier: Arc<Barrier>) -> (JoinHandle<()>, SocketAddr) {
    let (send, recv) = mpsc::channel();
    let handle = thread::spawn(move || {
        let listener = TcpListener::bind(address).expect("failed to bind address");
        let local_address = listener.local_addr().expect("failed to get local address");
        send.send(local_address)
            .expect("failed to send local address");

        // Unblock the first peer and accept connection
        barrier.wait();
        let (stream, _) = listener
            .accept()
            .expect("failed to accept first connection");

        // Wait for peer to close connection and drop stream
        barrier.wait();
        drop(stream);
    });
    (
        handle,
        recv.recv().expect("failed to receive local address"),
    )
}

fn start_write_shutdown_server(
    address: SocketAddr,
    barrier: Arc<Barrier>,
) -> (JoinHandle<()>, SocketAddr) {
    let (send, recv) = mpsc::channel();
    let handle = thread::spawn(move || {
        let listener = TcpListener::bind(address).expect("failed to bind address");
        let local_address = listener.local_addr().expect("failed to get local address");
        send.send(local_address)
            .expect("failed to send local address");

        // Unblock the first peer and accept connection
        barrier.wait();
        let (stream, _) = listener
            .accept()
            .expect("failed to accept first connection");

        // Immediately shutdown the write half of the socket--sending a FIN to the peer
        assert_ok!(stream.shutdown(Shutdown::Write));

        // Wait for the third peer to close connection and drop the stream
        barrier.wait();
        drop(stream);
    });
    (
        handle,
        recv.recv().expect("failed to receive local address"),
    )
}

fn register<S, CE, I, C, T>(
    poll: &mut Poll,
    events: &mut Events,
    source: &S,
    interests: Interests,
    create_event: CE,
    cleanup: C,
) -> Result<T>
where
    S: Source,
    CE: FnOnce() -> Result<I>,
    C: FnOnce() -> Result<T>,
{
    poll.registry().register(source, TOK, interests)?;
    create_event()?;
    expect_events(poll, events, vec![ExpectEvent::new(TOK, interests)]);
    poll.registry().deregister(source)?;
    cleanup()
}
