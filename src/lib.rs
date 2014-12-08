#![crate_name = "mio"]
#![feature(globs)]
#![feature(phase)]
#![feature(unsafe_destructor)]
// While in active dev
#![allow(dead_code)]

extern crate alloc;
extern crate nix;
extern crate time;

#[phase(plugin, link)]
extern crate log;

pub use buf::{
    Buf,
    MutBuf,
};
pub use error::{
    MioResult,
    MioError,
    MioErrorKind
};
pub use handler::{
    Handler,
};
pub use io::{
    pipe,
    NonBlock,
    IoReader,
    IoWriter,
    IoAcceptor,
    PipeReader,
    PipeWriter,
};
pub use poll::{
    Poll
};
pub use event_loop::{
    EventLoop,
    EventLoopConfig,
    EventLoopResult,
    EventLoopSender,
};
pub use timer::{
    Timeout,
};
pub use os::token::{
    Token,
};

pub use os::event;

pub mod buf;
pub mod net;
pub mod util;

mod error;
mod event_loop;
mod handler;
mod io;
mod notify;
mod os;
mod poll;
mod timer;
mod mpmc_bounded_queue;