//! A fast, low-level IO library for Rust focusing on non-blocking APIs, event
//! notification, and other useful utilities for building high performance IO
//! apps.
//!
//! # Goals
//!
//! * Fast - minimal overhead over the equivalent OS facilities (epoll, kqueue, etc...)
//! * Zero allocations
//! * A scalable readiness-based API, similar to epoll on Linux
//! * Design to allow for stack allocated buffers when possible (avoid double buffering).
//! * Provide utilities such as a timers, a notification channel, buffer abstractions, and a slab.
//!
//! # Usage
//!
//! Using mio starts by creating an [EventLoop](struct.EventLoop.html), which
//! handles receiving events from the OS and dispatching them to a supplied
//! [Handler](handler/trait.Handler.html).
//!
//! # Example
//!
//! ```
//! use mio::*;
//! use mio::tcp::{TcpListener, TcpStream};
//!
//! // Setup some tokens to allow us to identify which event is
//! // for which socket.
//! const SERVER: Token = Token(0);
//! const CLIENT: Token = Token(1);
//! #
//! # // level() isn't implemented on windows yet
//! # if cfg!(windows) { return }
//!
//! let addr = "127.0.0.1:13265".parse().unwrap();
//!
//! // Setup the server socket
//! let server = TcpListener::bind(&addr).unwrap();
//!
//! // Create an event loop
//! let mut event_loop = EventLoop::new().unwrap();
//!
//! // Start listening for incoming connections
//! event_loop.register(&server, SERVER, EventSet::readable(),
//!                     PollOpt::edge()).unwrap();
//!
//! // Setup the client socket
//! let sock = TcpStream::connect(&addr).unwrap();
//!
//! // Register the socket
//! event_loop.register(&sock, CLIENT, EventSet::readable(),
//!                     PollOpt::edge()).unwrap();
//!
//! // Define a handler to process the events
//! struct MyHandler(TcpListener);
//!
//! impl Handler for MyHandler {
//!     type Timeout = ();
//!     type Message = ();
//!
//!     fn ready(&mut self, event_loop: &mut EventLoop<MyHandler>, token: Token, _: EventSet) {
//!         match token {
//!             SERVER => {
//!                 let MyHandler(ref mut server) = *self;
//!                 // Accept and drop the socket immediately, this will close
//!                 // the socket and notify the client of the EOF.
//!                 let _ = server.accept();
//!             }
//!             CLIENT => {
//!                 // The server just shuts down the socket, let's just
//!                 // shutdown the event loop
//!                 event_loop.shutdown();
//!             }
//!             _ => panic!("unexpected token"),
//!         }
//!     }
//! }
//!
//! // Start handling events
//! event_loop.run(&mut MyHandler(server)).unwrap();
//!
//! ```

#![crate_name = "mio"]
#![cfg_attr(unix, deny(warnings))]

extern crate bytes;
extern crate time;
extern crate slab;
extern crate libc;

#[cfg(unix)]
extern crate nix;

extern crate winapi;
extern crate miow;
extern crate net2;

#[macro_use]
extern crate log;

#[cfg(test)]
extern crate env_logger;

pub mod util;

mod event;
mod event_loop;
mod handler;
mod io;
mod net;
mod notify;
mod poll;
mod sys;
mod timer;
mod token;

pub use event::{
    PollOpt,
    EventSet,
    IoEvent,
};
pub use event_loop::{
    EventLoop,
    EventLoopConfig,
    Sender,
};
pub use handler::{
    Handler,
};
pub use io::{
    TryRead,
    TryWrite,
    Evented,
    TryAccept,
};
pub use net::{
    tcp,
    udp,
    IpAddr,
    Ipv4Addr,
    Ipv6Addr,
};
#[cfg(unix)]
pub mod unix {
    pub use net::unix::{
        pipe,
        PipeReader,
        PipeWriter,
        UnixListener,
        UnixSocket,
        UnixStream,
    };
    pub use sys::{
        EventedFd,
    };
}

pub use notify::{
    NotifyError,
};
pub use poll::{
    Poll,
    Events,
};
pub use timer::{
    Timeout,
    TimerError,
    TimerResult
};
pub use token::{
    Token,
};
#[cfg(unix)]
pub use sys::Io;
pub use sys::Selector;

pub mod prelude {
    pub use super::{
        EventLoop,
        TryRead,
        TryWrite,
    };
}
