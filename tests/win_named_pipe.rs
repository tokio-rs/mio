#![cfg(windows)]

use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::*;
use std::os::windows::io::*;
use std::task::Poll;
use std::time::Duration;

// use mio::{Events, Poll, PollOpt, Ready, Token};
use mio::windows::NamedPipe;
// use rand::Rng;
use winapi::um::winbase::*;

use futures::executor::block_on;
use futures::future::poll_fn;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {}", stringify!($e), e),
        }
    };
}

fn server(registry: &mio::Registry) -> (NamedPipe, String) {
    let num: u64 = 188923014239;
    let name = format!(r"\\.\pipe\my-pipe-{}", num);
    let pipe = t!(NamedPipe::new(&name, registry, mio::Token(0)));
    (pipe, name)
}

fn client(name: &str, registry: &mio::Registry) -> NamedPipe {
    let mut opts = OpenOptions::new();
    opts.read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED);
    let file = t!(opts.open(name));
    t!(NamedPipe::from_raw_handle(
        file.into_raw_handle(),
        registry,
        mio::Token(1)
    ))
}

fn pipe(registry: &mio::Registry) -> (NamedPipe, NamedPipe) {
    let (pipe, name) = server(registry);
    (pipe, client(&name, registry))
}

#[test]
fn writable_after_register() {
    let mut poll = t!(mio::Poll::new());
    let (mut server, mut client) = pipe(poll.registry());
    let mut events = mio::Events::with_capacity(128);

    t!(poll.poll(&mut events, None));

    let events = events.iter().collect::<Vec<_>>();

    // Server is writable
    block_on(poll_fn(|cx| {
        let res = server.write(cx, b"hello");
        assert!(res.is_ready());
        res
    }))
    .unwrap();

    // Client is writable
    block_on(poll_fn(|cx| {
        let res = client.write(cx, b"hello");
        assert!(res.is_ready());
        res
    }))
    .unwrap();
}

/*
#[test]
fn write_then_read() {
    let mut poll = t!(mio::Poll::new());
    let (mut server, mut client) = pipe(poll.registry());
    let mut events = mio::Events::with_capacity(128);

    t!(poll.poll(&mut events, None));

    // Client is writable
    block_on(poll_fn(|cx| {
        let res = client.write(cx, b"1234");
        assert!(res.is_ready());
        res
    })).unwrap();

    loop {
        t!(poll.poll(&mut events, None));
        let events = events.iter().collect::<Vec<_>>();
        if let Some(event) = events.iter().find(|e| e.token() == Token(0)) {
            if event.readiness().is_readable() {
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
    drop(env_logger::init());

    let (server, name) = server();
    let poll = t!(Poll::new());
    t!(poll.register(
        &server,
        Token(0),
        Ready::readable() | Ready::writable(),
        PollOpt::edge()
    ));

    let mut events = Events::with_capacity(128);
    t!(poll.poll(&mut events, Some(Duration::new(0, 0))));
    let e = events.iter().collect::<Vec<_>>();
    assert_eq!(e.len(), 0);
    assert_eq!(
        server.connect().err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );

    let client = client(&name);
    t!(poll.register(
        &client,
        Token(1),
        Ready::readable() | Ready::writable(),
        PollOpt::edge()
    ));
    loop {
        t!(poll.poll(&mut events, None));
        let e = events.iter().collect::<Vec<_>>();
        if let Some(event) = e.iter().find(|e| e.token() == Token(0)) {
            if event.readiness().is_writable() {
                break;
            }
        }
    }
}

#[test]
fn connect_after_client() {
    drop(env_logger::init());

    let (server, name) = server();
    let poll = t!(Poll::new());
    t!(poll.register(
        &server,
        Token(0),
        Ready::readable() | Ready::writable(),
        PollOpt::edge()
    ));

    let mut events = Events::with_capacity(128);
    t!(poll.poll(&mut events, Some(Duration::new(0, 0))));
    let e = events.iter().collect::<Vec<_>>();
    assert_eq!(e.len(), 0);

    let client = client(&name);
    t!(poll.register(
        &client,
        Token(1),
        Ready::readable() | Ready::writable(),
        PollOpt::edge()
    ));
    t!(server.connect());
    loop {
        t!(poll.poll(&mut events, None));
        let e = events.iter().collect::<Vec<_>>();
        if let Some(event) = e.iter().find(|e| e.token() == Token(0)) {
            if event.readiness().is_writable() {
                break;
            }
        }
    }
}

#[test]
fn write_then_drop() {
    drop(env_logger::init());

    let (mut server, mut client) = pipe();
    let poll = t!(Poll::new());
    t!(poll.register(
        &server,
        Token(0),
        Ready::readable() | Ready::writable(),
        PollOpt::edge()
    ));
    t!(poll.register(
        &client,
        Token(1),
        Ready::readable() | Ready::writable(),
        PollOpt::edge()
    ));
    assert_eq!(t!(client.write(b"1234")), 4);
    drop(client);

    let mut events = Events::with_capacity(128);

    loop {
        t!(poll.poll(&mut events, None));
        let events = events.iter().collect::<Vec<_>>();
        if let Some(event) = events.iter().find(|e| e.token() == Token(0)) {
            if event.readiness().is_readable() {
                break;
            }
        }
    }

    let mut buf = [0; 10];
    assert_eq!(t!(server.read(&mut buf)), 4);
    assert_eq!(&buf[..4], b"1234");
}

#[test]
fn connect_twice() {
    drop(env_logger::init());

    let (mut server, name) = server();
    let c1 = client(&name);
    let poll = t!(Poll::new());
    t!(poll.register(
        &server,
        Token(0),
        Ready::readable() | Ready::writable(),
        PollOpt::edge()
    ));
    t!(poll.register(
        &c1,
        Token(1),
        Ready::readable() | Ready::writable(),
        PollOpt::edge()
    ));
    drop(c1);

    let mut events = Events::with_capacity(128);

    loop {
        t!(poll.poll(&mut events, None));
        let events = events.iter().collect::<Vec<_>>();
        if let Some(event) = events.iter().find(|e| e.token() == Token(0)) {
            if event.readiness().is_readable() {
                break;
            }
        }
    }

    let mut buf = [0; 10];
    assert_eq!(t!(server.read(&mut buf)), 0);
    t!(server.disconnect());
    assert_eq!(
        server.connect().err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );

    let c2 = client(&name);
    t!(poll.register(
        &c2,
        Token(2),
        Ready::readable() | Ready::writable(),
        PollOpt::edge()
    ));

    loop {
        t!(poll.poll(&mut events, None));
        let events = events.iter().collect::<Vec<_>>();
        if let Some(event) = events.iter().find(|e| e.token() == Token(0)) {
            if event.readiness().is_writable() {
                break;
            }
        }
    }
}
*/
