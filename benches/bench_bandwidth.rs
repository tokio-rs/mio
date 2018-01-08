#![feature(test)]

extern crate mio;
extern crate test;

use test::Bencher;

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

const TOTAL_BYTES: usize = 32*1024*1024; // 16MB

/// Benchmarking single-threaded transfer of TOTAL_BYTES from N clients to a single reader
///
/// The number of bytes transfered is the same for all set-ups, and benchmarks can be compared
/// with each other. In case of multiple clients N, each client will send identical quota
/// TOTAL_BYTES/N to the reader.
fn read_bytes_from_nclients(bench: &mut Bencher, txbuf: &[u8], rxbuf: &mut [u8], nclients: usize) {
    const MS: u64 = 100;

    const LISTENER: usize = 255;

    let mut poll = Poll::new().unwrap();
    let mut pevents = Events::with_capacity(1024);

    let mut txset = Vec::with_capacity(nclients);
    let mut rxset = Vec::with_capacity(nclients);

    // Create the listener
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();

    // Register the listener with `Poll`
    poll.register().register(&l, Token(LISTENER), Ready::readable(), PollOpt::edge()).unwrap();


    for id in 0..nclients {
        let txtok = id;
        let rxtok = id + nclients;

        let mut tx: Box<TcpStream> = Box::new(TcpStream::connect(&l.local_addr().unwrap()).unwrap());
        poll.register().register(&*tx, Token(txtok),  Ready::writable(), PollOpt::level()).unwrap();
        txset.push(tx);

        expect_events(&mut poll, &mut pevents, 2, vec![
            Event::new(Ready::readable(), Token(LISTENER))]);

        let mut rx = Box::new(l.accept().unwrap().0);
        poll.register().register(&*rx, Token(rxtok), Ready::readable(), PollOpt::level()).unwrap();
        rxset.push(rx);

        expect_events(&mut poll, &mut pevents, 2, vec![
            Event::new(Ready::writable(), Token(txtok))]);
    }

    // Sleep a bit to ensure it arrives at dest
    sleep_ms(250);

    bench.iter(move || {
        let mut rxtotal: usize = 0;
        let mut rxsum = vec![0usize; nclients];
        let mut txsum = vec![0usize; nclients];

        for id in 0..nclients {
            // Reregister as writable in case the previous test did reregister as none
            poll.register().reregister(&*txset[id], Token(id), Ready::writable(), PollOpt::level()).unwrap();
        }


        while rxtotal < TOTAL_BYTES {
            poll.poll(&mut pevents, Some(Duration::from_millis(MS))).unwrap();

            for event in pevents.iter() {
                match event.token() {
                    Token(tok) if tok < nclients => {
                        let id = tok;
                        let start = txsum[id] % txbuf.len();
                        match (*txset[id]).write(&txbuf[start..]) {
                            Ok(nwritten) => txsum[id] += nwritten,
                            e => panic!("failed to write data {:?}", e)
                        }

                        // No more TX if intended quota of connection reached
                        if txsum[id] >= TOTAL_BYTES/nclients {
                            poll.register().reregister(&*txset[id], Token(id),
                                                       Ready::empty(), PollOpt::level()).unwrap();
                        }
                    },

                    Token(tok) if tok >= nclients && tok < 2*nclients => {
                        let id = tok - nclients;

                        match (*rxset[id]).read (rxbuf) {
                            Ok(nread) => {
                                rxsum[id] += nread;
                                rxtotal += nread;
                            },
                            _ => panic!("failed to read data")
                        };
                    },

                    Token(LISTENER) =>  panic!("unexpected event"),
                    _ =>  panic!("unexpected event")
                }
            }
        }
    });
}

/// ------------------ 8 clients ----------------------

/// Benchmarking bandwidth using 8 sendclients and buffer-size 512
#[bench]
fn bench_bandwidth_st_writer_08_00512(bench: &mut Bencher) {

    const BUFLEN: usize = 512;
    const NCLIENTS: usize = 8;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 8 sendclients and buffer-size 1024
#[bench]
fn bench_bandwidth_st_writer_08_01024(bench: &mut Bencher) {

    const BUFLEN: usize = 1024;
    const NCLIENTS: usize = 8;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 8 send-clients and buffer-size 2048
#[bench]
fn bench_bandwidth_st_writer_08_02048(bench: &mut Bencher) {

    const BUFLEN: usize = 2048;
    const NCLIENTS: usize = 8;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 8 send-clients and buffer-size 4096
#[bench]
fn bench_bandwidth_st_writer_08_04096(bench: &mut Bencher) {

    const BUFLEN: usize = 4096;
    const NCLIENTS: usize = 8;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 8 send-clients and buffer-size 8192
#[bench]
fn bench_bandwidth_st_writer_08_08192(bench: &mut Bencher) {

    const BUFLEN: usize = 8192;
    const NCLIENTS: usize = 8;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 8 send-clients and buffer-size 16384
#[bench]
fn bench_bandwidth_st_writer_08_16384(bench: &mut Bencher) {

    const BUFLEN: usize = 16384;
    const NCLIENTS: usize = 8;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 8 send-clients and buffer-size 32768
#[bench]
fn bench_bandwidth_st_writer_08_32768(bench: &mut Bencher) {

    const BUFLEN: usize = 32768;
    const NCLIENTS: usize = 8;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}


/// Benchmarking bandwidth using 8 send-clients and buffer-size 65536
#[bench]
fn bench_bandwidth_st_writer_08_65536(bench: &mut Bencher) {

    const BUFLEN: usize = 65536;
    const NCLIENTS: usize = 8;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}


/// ------------------ 4 clients ----------------------

/// Benchmarking bandwidth using 4 sendclients and buffer-size 512
#[bench]
fn bench_bandwidth_st_writer_04_00512(bench: &mut Bencher) {

    const BUFLEN: usize = 512;
    const NCLIENTS: usize = 4;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 4 sendclients and buffer-size 1024
#[bench]
fn bench_bandwidth_st_writer_04_01024(bench: &mut Bencher) {

    const BUFLEN: usize = 1024;
    const NCLIENTS: usize = 4;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 4 send-clients and buffer-size 512
#[bench]
fn bench_bandwidth_st_writer_04_02048(bench: &mut Bencher) {

    const BUFLEN: usize = 2048;
    const NCLIENTS: usize = 4;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 4 send-clients and buffer-size 4096
#[bench]
fn bench_bandwidth_st_writer_04_04096(bench: &mut Bencher) {

    const BUFLEN: usize = 4096;
    const NCLIENTS: usize = 4;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 4 send-clients and buffer-size 8192
#[bench]
fn bench_bandwidth_st_writer_04_08192(bench: &mut Bencher) {

    const BUFLEN: usize = 8192;
    const NCLIENTS: usize = 4;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 4 send-clients and buffer-size 16384
#[bench]
fn bench_bandwidth_st_writer_04_16384(bench: &mut Bencher) {

    const BUFLEN: usize = 16384;
    const NCLIENTS: usize = 4;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 4 send-clients and buffer-size 32768
#[bench]
fn bench_bandwidth_st_writer_04_32768(bench: &mut Bencher) {

    const BUFLEN: usize = 32768;
    const NCLIENTS: usize = 4;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}


/// Benchmarking bandwidth using 4 send-clients and buffer-size 65536
#[bench]
fn bench_bandwidth_st_writer_04_65536(bench: &mut Bencher) {

    const BUFLEN: usize = 65536;
    const NCLIENTS: usize = 4;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}


/// ------------------ 2 clients ----------------------

/// Benchmarking bandwidth using 2 sendclients and buffer-size 512
#[bench]
fn bench_bandwidth_st_writer_02_00512(bench: &mut Bencher) {

    const BUFLEN: usize = 512;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 2 sendclients and buffer-size 1024
#[bench]
fn bench_bandwidth_st_writer_02_01024(bench: &mut Bencher) {

    const BUFLEN: usize = 1024;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 2 send-clients and buffer-size 2048
#[bench]
fn bench_bandwidth_st_writer_02_02048(bench: &mut Bencher) {

    const BUFLEN: usize = 2048;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 2 send-clients and buffer-size 4096
#[bench]
fn bench_bandwidth_st_writer_02_04096(bench: &mut Bencher) {

    const BUFLEN: usize = 4096;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 2 send-clients and buffer-size 8192
#[bench]
fn bench_bandwidth_st_writer_02_08192(bench: &mut Bencher) {

    const BUFLEN: usize = 8192;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 2 send-clients and buffer-size 16384
#[bench]
fn bench_bandwidth_st_writer_02_16384(bench: &mut Bencher) {

    const BUFLEN: usize = 16384;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 2 send-clients and buffer-size 32768
#[bench]
fn bench_bandwidth_st_writer_02_32768(bench: &mut Bencher) {

    const BUFLEN: usize = 32768;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}


/// Benchmarking bandwidth using 2 send-clients and buffer-size 65536
#[bench]
fn bench_bandwidth_st_writer_02_65536(bench: &mut Bencher) {

    const BUFLEN: usize = 65536;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// ------------------ 1 client ----------------------

/// Benchmarking bandwidth using 1 send-client and buffer-size 512
#[bench]
fn bench_bandwidth_st_writer_01_00512(bench: &mut Bencher) {

    const BUFLEN: usize = 512;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 1 send-client and buffer-size 1024
#[bench]
fn bench_bandwidth_st_writer_01_01024(bench: &mut Bencher) {

    const BUFLEN: usize = 1024;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 1 send-client and buffer-size 2048

#[bench]
fn bench_bandwidth_st_writer_01_02048(bench: &mut Bencher) {

    const BUFLEN: usize = 2048;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 1 send-client and buffer-size 4096
#[bench]
fn bench_bandwidth_st_writer_01_04096(bench: &mut Bencher) {

    const BUFLEN: usize = 4096;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 1 send-client and buffer-size 8192
#[bench]
fn bench_bandwidth_st_writer_01_08192(bench: &mut Bencher) {

    const BUFLEN: usize = 8192;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 1 send-client and buffer-size 16384
#[bench]
fn bench_bandwidth_st_writer_01_16384(bench: &mut Bencher) {

    const BUFLEN: usize = 16384;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}

/// Benchmarking bandwidth using 1 send-client and buffer-size 32768
#[bench]
fn bench_bandwidth_st_writer_01_32768(bench: &mut Bencher) {

    const BUFLEN: usize = 32768;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}


/// Benchmarking bandwidth using 1 send-client and buffer-size 65536
#[bench]
fn bench_bandwidth_st_writer_01_65536(bench: &mut Bencher) {

    const BUFLEN: usize = 65536;
    const NCLIENTS: usize = 2;

    let txbuf: [u8; BUFLEN] = [42; BUFLEN];
    let mut rxbuf: [u8; BUFLEN] = unsafe { ::std::mem::uninitialized() };

    read_bytes_from_nclients(bench, &txbuf[0..], &mut rxbuf[0..], NCLIENTS);
}