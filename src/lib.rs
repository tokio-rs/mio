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
//! use mio::tcp::TcpListener;
//! use std::net::SocketAddr;
//! use std::str::FromStr;
//!
//! // Setup some tokens to allow us to identify which event is
//! // for which socket.
//! const SERVER: Token = Token(0);
//! const CLIENT: Token = Token(1);
//!
//! let addr = FromStr::from_str("127.0.0.1:13265").unwrap();
//!
//! // Setup the server socket
//! let server = tcp::listen(&addr).unwrap();
//!
//! // Create an event loop
//! let mut event_loop = EventLoop::new().unwrap();
//!
//! // Start listening for incoming connections
//! event_loop.register(&server, SERVER).unwrap();
//!
//! // Setup the client socket
//! let (sock, _) = tcp::connect(&addr).unwrap();
//!
//! // Register the socket
//! event_loop.register(&sock, CLIENT).unwrap();
//!
//! // Define a handler to process the events
//! struct MyHandler(NonBlock<TcpListener>);
//!
//! impl Handler for MyHandler {
//!     type Timeout = ();
//!     type Message = ();
//!
//!     fn readable(&mut self, event_loop: &mut EventLoop<MyHandler>, token: Token, _: ReadHint) {
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

// mio is still in rapid development
#![unstable]

#![feature(alloc, core, io, libc, net, std_misc, unsafe_destructor)]

extern crate alloc;
extern crate bytes;
extern crate nix;
extern crate time;

#[macro_use]
extern crate log;

pub mod util;

mod event_loop;
mod handler;
mod io;
mod net;
mod nonblock;
mod notify;
mod os;
mod poll;
mod timer;

pub use buf::{
    Buf,
    MutBuf,
};
pub use event_loop::{
    EventLoop,
    EventLoopConfig,
    EventLoopSender,
};
pub use handler::{
    Handler,
};
pub use io::{
    pipe,
    FromFd,
    Io,
    TryRead,
    TryWrite,
    Evented,
    PipeReader,
    PipeWriter,
};
pub use net::{
    tcp,
    udp,
    unix,
    Socket,
};
pub use nonblock::{
    IntoNonBlock,
    NonBlock,
};
pub use notify::{
    NotifyError,
};
pub use os::token::{
    Token,
};
pub use os::event::{
    PollOpt,
    Interest,
    ReadHint,
};
pub use poll::{
    Poll
};
pub use timer::{
    Timeout,
    TimerError,
    TimerResult
};

pub mod prelude {
    pub use super::{
        EventLoop,
        TryRead,
        TryWrite,
    };
}

// Re-export bytes
pub mod buf {
    pub use bytes::{
        Buf,
        MutBuf,
        ByteBuf,
        MutByteBuf,
        RingBuf,
        SliceBuf,
        MutSliceBuf,
    };
}
