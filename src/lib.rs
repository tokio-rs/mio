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
pub use handler::Handler;
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
pub use reactor::{
    Reactor,
    ReactorConfig,
    ReactorResult,
    ReactorSender,
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
mod handler;
mod io;
mod notify;
mod os;
mod poll;
mod reactor;
mod slab;
mod socket;
mod timer;
mod token;
