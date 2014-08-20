#![crate_name = "mio"]
#![feature(globs)]
#![feature(unsafe_destructor)]

extern crate alloc;
extern crate nix;

pub use handler::Handler;
pub use reactor::{Reactor, IoHandle};
pub use sock::{TcpSocket, SockAddr};

mod error;
mod handler;
mod reactor;
mod sock;
mod timer;
mod util;

#[cfg(target_os = "linux")]
#[path = "os_linux.rs"]
mod os;
