#![cfg(windows)]

use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::*;
use std::os::windows::io::*;
use std::task::{Context, Poll};
use std::time::Duration;

// use mio::{Events, Poll, PollOpt, Ready, Token};
use mio::windows::NamedPipe;
use rand::Rng;
use winapi::um::winbase::*;

use futures_test::task::new_count_waker;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {}", stringify!($e), e),
        }
    };
}

fn server(registry: &mio::Registry) -> (NamedPipe, String) {
    let num: u64 = rand::thread_rng().gen();
    let name = format!(r"\\.\pipe\my-pipe-{}", num);
    let pipe = t!(NamedPipe::new(&name, registry));
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
    ))
}

fn pipe(registry: &mio::Registry) -> (NamedPipe, NamedPipe) {
    let (pipe, name) = server(registry);
    (pipe, client(&name, registry))
}

static data: &[u8] = &[100; 4096];

#[test]
fn writable_after_register() {
    println!("START");
    let mut poll = t!(mio::Poll::new());
    println!("NEXT");
    let (mut server, mut client) = pipe(poll.registry());
    let mut events = mio::Events::with_capacity(128);
    println!("prepoll");

    println!("one");

    let (wk1, cnt1) = new_count_waker();
    let mut cx1 = Context::from_waker(&wk1);
    let (wk2, cnt2) = new_count_waker();
    let mut cx2 = Context::from_waker(&wk2);

    let mut dst = [0; 1024];

    t!(server.connect());

    println!("two");

    // Server is writable
    let res = server.write(&mut cx2, b"hello");
    assert!(res.is_ready());

    // Server is **not** readable
    assert!(server.read(&mut cx2, &mut dst).is_pending());

    println!("three");

    // Client is writable
    let res = client.write(&mut cx1, b"hello");
    println!("  -> 1");
    assert!(res.is_ready());

    // Saturate the client
    loop {
        println!("  -> 2");
        if client.write(&mut cx1, data).is_pending() {
            break;
        }
    }

    println!(" => loop 2");

    // Wait for readable
    while cnt2.get() == 0 {
        t!(poll.poll(&mut events, None));
    }

    // Read some data
    let mut n = 0;

    while server.read(&mut cx2, &mut dst).is_ready() {
        n += 1;
    }

    assert!(n > 0);

    // Wait for the write side to be notified
    while cnt1.get() == 0 {
        t!(poll.poll(&mut events, None));
    }
}

#[test]
fn connect_before_client() {
    let mut poll = t!(mio::Poll::new());

    let (server, name) = server(poll.registry());

    let mut events = mio::Events::with_capacity(128);
    t!(poll.poll(&mut events, Some(Duration::new(0, 0))));
    println!(" ~~~ done poll ");
    let e = events.iter().collect::<Vec<_>>();
    assert_eq!(e.len(), 0);
    assert_eq!(
        server.connect().err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );

    let mut client = client(&name, poll.registry());    
    let (wk, cnt) = new_count_waker();
    let mut cx = Context::from_waker(&wk);

    assert!(client.write(&mut cx, b"hello").is_ready());
}

#[test]
fn connect_after_client() {
    let mut poll = t!(mio::Poll::new());

    let (server, name) = server(poll.registry());

    let mut events = mio::Events::with_capacity(128);
    t!(poll.poll(&mut events, Some(Duration::new(0, 0))));
    println!(" ~~~ done poll ");
    let e = events.iter().collect::<Vec<_>>();
    assert_eq!(e.len(), 0);

    let mut client = client(&name, poll.registry());    
    let (wk, cnt) = new_count_waker();
    let mut cx = Context::from_waker(&wk);

    assert!(server.connect().is_ok());

    assert!(client.write(&mut cx, b"hello").is_ready());
}

#[test]
fn write_then_drop() {
    let mut poll = t!(mio::Poll::new());
    let (mut server, mut client) = pipe(poll.registry());

    let (wk1, cnt1) = new_count_waker();
    let mut cx1 = Context::from_waker(&wk1);
    let (wk2, cnt2) = new_count_waker();
    let mut cx2 = Context::from_waker(&wk2);

    t!(server.connect());

    let mut dst = [0; 1024];

    assert!(server.read(&mut cx2, &mut dst).is_pending());

    match client.write(&mut cx1, b"1234") {
        Poll::Ready(res) => assert_eq!(t!(res), 4),
        _ => panic!(),
    }

    drop(client);

    let mut events = mio::Events::with_capacity(128);

    // Wait for readable
    while cnt2.get() == 0 {
        t!(poll.poll(&mut events, None));
    }

    match server.read(&mut cx2, &mut dst) {
        Poll::Ready(res) => assert_eq!(t!(res), 4),
        _ => panic!(),
    }

    assert_eq!(&dst[..4], b"1234");
}

#[test]
fn connect_twice() {
    let mut poll = t!(mio::Poll::new());

    let (mut server, name) = server(poll.registry());
    let mut c1 = client(&name, poll.registry());

    let (wk1, cnt1) = new_count_waker();
    let mut cx1 = Context::from_waker(&wk1);

    let mut dst = [0; 1024];
    assert!(server.read(&mut cx1, &mut dst).is_pending());

    t!(server.connect());

    drop(c1);
    let mut events = mio::Events::with_capacity(128);

    while cnt1.get() == 0 {
        t!(poll.poll(&mut events, None));
    }

    match server.read(&mut cx1, &mut dst) {
        Poll::Ready(Ok(0)) => {}
        res => panic!("{:?}", res),
    }

    t!(server.disconnect());
    assert_eq!(
        server.connect().err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );

    assert_eq!(
        server.connect().err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );

    assert!(server.write(&mut cx1, b"hello").is_ready());
    assert!(server.write(&mut cx1, b"hello").is_pending());
}
