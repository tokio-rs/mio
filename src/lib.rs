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

pub use error::{MioResult, MioError};
pub use handler::Handler;
pub use io::{IoReader, IoWriter, IoAcceptor, TcpSocket, TcpAcceptor, UnixSocket, SockAddr};
pub use reactor::{Reactor};

mod error;
mod handler;
mod io;
mod os;
mod reactor;
mod timer;
mod util;
