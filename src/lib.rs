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

pub use error::MioResult;
pub use handler::Handler;
pub use reactor::{Reactor, IoHandle};
pub use sock::{TcpSocket, SockAddr};

mod error;
mod handler;
mod os;
mod reactor;
mod sock;
mod timer;
mod util;
