extern crate mio;

use mio::{EventLoop, EventLoopConfig, Handler, Token, EventSet, PollOpt};
use mio::tcp::TcpListener;

struct E;

impl Handler for E {
    type Timeout = ();
    type Message = ();
}

#[test]
fn reregister_before_register() {
    let mut e = EventLoop::<E>::new().unwrap();

    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();
    let res = e.reregister(&l, Token(1), EventSet::all(), PollOpt::edge());
    if cfg!(target_os = "macos") || cfg!(target_os = "freebsd") || cfg!(target_os = "dragonfly") {
        assert!(res.is_ok());
    } else {
        assert!(res.is_err());
    }
}

#[test]
fn run_once_with_nothing() {
    let mut config = EventLoopConfig::new();
    config.io_poll_timeout_ms(Some(10));
    let mut e = EventLoop::<E>::configured(config).unwrap();
    e.run_once(&mut E).unwrap();
}

#[test]
fn add_then_drop() {
    let mut config = EventLoopConfig::new();
    config.io_poll_timeout_ms(Some(10));
    let mut e = EventLoop::<E>::configured(config).unwrap();
    let l = TcpListener::bind(&"127.0.0.1:0".parse().unwrap()).unwrap();
    e.register(&l, Token(1), EventSet::all(), PollOpt::edge()).unwrap();
    drop(l);
    e.run_once(&mut E).unwrap();
}
