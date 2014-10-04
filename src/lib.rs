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
};
pub use handler::{
    Handler,
    ReadHint
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
    Poll,
    IoEvent,
    IoEventKind,
};
pub use event_loop::{
    EventLoop,
    EventLoopConfig,
    EventLoopResult,
    EventLoopSender,
};
pub use slab::Slab;
pub use socket::{
    Socket,
    SockAddr,
    TcpSocket,
    TcpAcceptor,
    UnixSocket,
};
pub use timer::{
    Timer,
    Timeout,
};
pub use token::{
    Token,
    TOKEN_0,
    TOKEN_1,
};

pub mod buf;
mod error;
mod event_loop;
pub mod handler;
mod io;
mod notify;
mod os;
mod poll;
mod slab;
mod socket;
mod timer;
mod token;
