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
pub use io::{IoReader, IoWriter, IoAcceptor};
pub use reactor::{Reactor};
pub use sock::{TcpSocket, TcpAcceptor, UnixSocket, SockAddr};

mod error;
mod handler;
mod io;
mod os;
mod reactor;
mod sock;
mod timer;
mod util;
