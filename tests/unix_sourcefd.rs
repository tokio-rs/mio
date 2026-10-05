#![cfg(all(
    unix,
    feature = "net",
    feature = "os-poll",
    feature = "os-ext",
    not(miri)
))] // Miri doesn't support Unix domain sockets.
#![cfg(not(target_os = "emscripten"))] // Emscripten has no socketpair(2).

mod util;

use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};

use util::{expect_events, expect_no_events, ExpectEvent, Readiness};

#[test]
fn source_fd_readable_after_would_block() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    let fd = reader.as_raw_fd();
    poll.registry()
        .register(&mut SourceFd(&fd), Token(0), Interest::READABLE)
        .unwrap();

    for _ in 0..3 {
        writer.write_all(b"x").unwrap();
        expect_events(
            &mut poll,
            &mut events,
            vec![ExpectEvent::new(Token(0), Readiness::READABLE)],
        );
        let mut buf = [0; 1];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"x");
        assert_eq!(
            reader.read(&mut buf).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        expect_no_events(&mut poll, &mut events);
    }

    poll.registry().deregister(&mut SourceFd(&fd)).unwrap();
    writer.write_all(b"x").unwrap();
    expect_no_events(&mut poll, &mut events);
}

#[test]
fn source_fd_writable_after_would_block() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    writer.set_nonblocking(true).unwrap();
    let fd = writer.as_raw_fd();
    poll.registry()
        .register(&mut SourceFd(&fd), Token(0), Interest::READABLE)
        .unwrap();
    poll.registry()
        .reregister(&mut SourceFd(&fd), Token(1), Interest::WRITABLE)
        .unwrap();

    for _ in 0..3 {
        expect_events(
            &mut poll,
            &mut events,
            vec![ExpectEvent::new(Token(1), Readiness::WRITABLE)],
        );
        let mut buf = [0; 4096];
        loop {
            match writer.write(&buf) {
                Ok(n) => assert!(n > 0),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => panic!("unexpected write error: {err}"),
            }
        }
        expect_no_events(&mut poll, &mut events);
        loop {
            match reader.read(&mut buf) {
                Ok(n) => assert!(n > 0),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => panic!("unexpected read error: {err}"),
            }
        }
    }

    poll.registry().deregister(&mut SourceFd(&fd)).unwrap();
    expect_no_events(&mut poll, &mut events);
}
