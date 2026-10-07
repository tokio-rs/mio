#![cfg(all(
    unix,
    any(feature = "net", all(feature = "os-poll", feature = "os-ext"))
))]

use mio::{Events, Interest, IoSource, Poll, Token};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

#[test]
fn io_source_rearm_on_would_block() {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);

    let (stream1, mut stream2) = UnixStream::pair().unwrap();
    stream1.set_nonblocking(true).unwrap();
    stream2.set_nonblocking(true).unwrap();

    let mut source = IoSource::new(stream1);
    poll.registry()
        .register(&mut source, Token(0), Interest::READABLE)
        .unwrap();

    let mut buf = [0u8; 16];
    let err = source.do_io(|mut s| s.read(&mut buf)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);

    // Write chunk 1
    stream2.write_all(b"chunk1").unwrap();
    poll.poll(&mut events, None).unwrap();
    assert!(!events.is_empty());

    let n = source.do_io(|mut s| s.read(&mut buf)).unwrap();
    assert_eq!(&buf[..n], b"chunk1");

    // Read again -> WouldBlock (re-arms in poll backend)
    let err = source.do_io(|mut s| s.read(&mut buf)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);

    // Write chunk 2
    stream2.write_all(b"chunk2").unwrap();
    poll.poll(&mut events, None).unwrap();
    assert!(!events.is_empty());

    let n = source.do_io(|mut s| s.read(&mut buf)).unwrap();
    assert_eq!(&buf[..n], b"chunk2");
}
