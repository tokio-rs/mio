#![feature(test)]

extern crate mio;
extern crate test;

use mio::*;
use test::Bencher;
use std::sync::Arc;
use std::thread;

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
                n += events.iter().count();
            }
        }
    })
}
