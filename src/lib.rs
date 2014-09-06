#![crate_name = "mio"]
#![feature(globs)]
#![feature(phase)]
#![feature(unsafe_destructor)]
// While in active dev
#![allow(dead_code)]

extern crate alloc;
extern crate nix;

#[phase(plugin, link)]
extern crate log;

pub use buf::{Buf, MutBuf};
pub use error::{MioResult, MioError};
pub use token::Token;
pub use io::{IoReader, IoWriter, IoAcceptor, Socket, TcpSocket, TcpAcceptor, UnixSocket, SockAddr};
pub use reactor::Reactor;
pub use slab::Slab;

pub mod buf;
mod error;
mod token;
mod event;
mod io;
mod os;
mod reactor;
mod slab;
mod timer;
