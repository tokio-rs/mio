#![feature(test)]
#![allow(dead_code)]
#![allow(unused_imports)]

extern crate mio;
extern crate test;

use mio::*;
use test::Bencher;
use std::sync::Arc;
use std::thread;

use mio::{Events, Poll, PollOpt, Ready, Token};
use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use std::io::{Write, Read};
use std::time::Duration;

fn expect_events(poll: &mut Poll,
                     event_buffer: &mut Events,
                     poll_try_count: usize,
                     mut expected: Vec<Event>)
{
    const MS: u64 = 1_000;

    for _ in 0..poll_try_count {
        poll.poll(event_buffer, Some(Duration::from_millis(MS))).unwrap();
        for event in event_buffer.iter() {
            let pos_opt = match expected.iter().position(|exp_event| {
                (event.token() == exp_event.token()) &&
                event.readiness().contains(exp_event.readiness())
            }) {
                Some(x) => Some(x),
                None => None,
            };
            if let Some(pos) = pos_opt { expected.remove(pos); }
        }

        if expected.len() == 0 {
            break;
        }
    }

    assert!(expected.len() == 0, "The following expected events were not found: {:?}", expected);
}


pub fn sleep_ms(ms: u64) {
    use std::thread;
    use std::time::Duration;
    thread::sleep(Duration::from_millis(ms));
}


#[bench]
fn bench_poll(bench: &mut Bencher) {
    const NUM: usize = 10_000;
    const THREADS: usize = 4;

    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(1024);

    let mut registrations = vec![];
    let mut set_readiness = vec![];

    for i in 0..NUM {
        let (r, s) = Registration::new();

         poll.register()
            .register(&r, Token(i), Ready::readable(), PollOpt::edge()).unwrap();

        registrations.push(r);
        set_readiness.push(s);
    }

    let set_readiness = Arc::new(set_readiness);

    bench.iter(move || {
        for mut i in 0..THREADS {
            let set_readiness = set_readiness.clone();
            thread::spawn(move || {
                while i < NUM {
                    set_readiness[i].set_readiness(Ready::readable()).unwrap();
                    i += THREADS;
                }
            });
        }

        let mut n: usize = 0;

        while n < NUM {
            if poll.poll(&mut events, None).is_ok() {
                // TBD: would be handy to have a method such as events.len()
                for _ in events.iter() {
                    n += 1;
                }
            }
        }
    })
}

///
///
fn bench_bandwidth_st_gen(bench: &mut Bencher, txbuf: &[u8], rxbuf: &mut [u8]) {
    const MS: u64 = 100;

    const LISTENER: usize = 0;
    const TX: usize = 1;
    const RX: usize = 2;

    let mut poll = Poll::new().unwrap();
    let mut pevents = Events::with_capacity(1024);

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register().register(&l, Token(0), Ready::readable(), PollOpt::edge()).unwrap();

    // Register the Transmitter TX
    let mut tx = TcpStream::connect(&l.local_addr().unwrap()).unwrap();

    poll.register().register(&tx, Token(1),  Ready::writable(), PollOpt::level()).unwrap();

    // Sleep a bit to ensure it arrives at dest
    sleep_ms(250);

    expect_events(&mut poll, &mut pevents, 2, vec![
        Event::new(Ready::readable(), Token(LISTENER)),
        Event::new(Ready::writable(), Token(TX)),
    ]);

    // Register the Receiver RX
    let (mut rx, _) = l.accept().unwrap();
    poll.register().register(&rx, Token(RX), Ready::readable(), PollOpt::level()).unwrap();

    // Sleep a bit to ensure it arrives at dest
    sleep_ms(250);

    let  ntimes: usize = TOTAL / txbuf.len();

    bench.iter(move || {
        let mut rxtotal: usize = 0;
        let mut txtotal: usize = 0;

        while rxtotal < ntimes {

            poll.register().reregister(&tx, Token(TX),  Ready::writable(), PollOpt::level()).unwrap();

            poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();

            for event in pevents.iter() {
                match event.token() {
                    Token(TX) => {
                        let start = txtotal % txbuf.len();
                        match tx.write(&txbuf[start..]) {
                            Ok(nwritten) => txtotal += nwritten,
                            _ => panic!("failed to write data")
                        }
                        if txtotal >= TOTAL {
                            poll.register().reregister(&tx, Token(TX),  Ready::empty(), PollOpt::level()).unwrap();
                        }
                    },
                    Token(RX) => {
                        match rx.read (rxbuf) {
                            Ok(nread) => rxtotal += nread,
                            _ => panic!("failed to read data")
                        }
                    },
                    _ =>  panic!("unexpected event")
                }
            }
        }
    });
}

const TOTAL: usize = 256*1024*1024; // 128MB
///
///
#[bench]
fn bench_bandwidth_st_writer_01_01024(bench: &mut Bencher) {

    const BUFLEN: usize = 1024; // 16KB

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    bench_bandwidth_st_gen(bench, &txbuf[0..], &mut rxbuf[0..]);
}

///
///
#[bench]
fn bench_bandwidth_st_writer_01_02048(bench: &mut Bencher) {

    const BUFLEN: usize = 2048; // 16KB

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    bench_bandwidth_st_gen(bench, &txbuf[0..], &mut rxbuf[0..]);
}

///
///
#[bench]
fn bench_bandwidth_st_writer_01_04096(bench: &mut Bencher) {

    const BUFLEN: usize = 4096; // 16KB

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    bench_bandwidth_st_gen(bench, &txbuf[0..], &mut rxbuf[0..]);
}

///
///
#[bench]
fn bench_bandwidth_st_writer_01_08192(bench: &mut Bencher) {

    const BUFLEN: usize = 8192; // 16KB

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

   bench_bandwidth_st_gen(bench, &txbuf[0..], &mut rxbuf[0..]);
}

///
///
#[bench]
fn bench_bandwidth_st_writer_01_16384(bench: &mut Bencher) {

    const BUFLEN: usize = 16384; // 16KB

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

   bench_bandwidth_st_gen(bench, &txbuf[0..], &mut rxbuf[0..]);
}

///
///
#[bench]
fn bench_bandwidth_st_writer_01_32768(bench: &mut Bencher) {

    const BUFLEN: usize = 32768; // 16KB

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

   bench_bandwidth_st_gen(bench, &txbuf[0..], &mut rxbuf[0..]);
}


///
///
#[bench]
fn bench_bandwidth_st_writer_01_65536(bench: &mut Bencher) {

    const BUFLEN: usize = 65536; // 16KB

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

   bench_bandwidth_st_gen(bench, &txbuf[0..], &mut rxbuf[0..]);
}