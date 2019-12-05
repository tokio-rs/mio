#![allow(warnings)]

macro_rules! os_required {
    () => {
        panic!("mio must be compiled with `os-poll` to run.")
    };
}

mod selector;
pub(crate) use self::selector::{event, Event, Events, Selector};

cfg_tcp! {
    mod tcp;
    pub(crate) use self::tcp::{TcpStream, TcpListener};
}

mod waker;
pub(crate) use self::waker::Waker;
