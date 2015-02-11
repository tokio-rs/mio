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
//! use mio::{event, EventLoop, IoAcceptor, Handler, Token};
//! use mio::net::{SockAddr};
//! use mio::net::tcp::{TcpSocket, TcpAcceptor};
//!
//! // Setup some tokens to allow us to identify which event is
//! // for which socket.
//! const SERVER: Token = Token(0);
//! const CLIENT: Token = Token(1);
//!
//! let addr = SockAddr::parse("127.0.0.1:13265").unwrap();
//!
//! // Setup the server socket
//! let server = TcpSocket::v4().unwrap()
//!     .bind(&addr).unwrap()
//!     .listen(256).unwrap();
//!
//! // Create an event loop
//! let mut event_loop = EventLoop::<(), ()>::new().unwrap();
//!
//! // Start listening for incoming connections
//! event_loop.register(&server, SERVER).unwrap();
//!
//! // Setup the client socket
//! let sock = TcpSocket::v4().unwrap();
//!
//! // Connect to the server
//! sock.connect(&addr).unwrap();
//!
//! // Register the socket
//! event_loop.register(&sock, CLIENT).unwrap();
//!
//! // Define a handler to process the events
//! struct MyHandler(TcpAcceptor);
//!
//! impl Handler<(), ()> for MyHandler {
//!     fn readable(&mut self, event_loop: &mut EventLoop<(), ()>, token: Token, _: event::ReadHint) {
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
//! let _ = event_loop.run(MyHandler(server));
//!
//! ```

#![crate_name = "mio"]

// mio is still in rapid development
#![unstable]

#![feature(unsafe_destructor, alloc, core, io, libc, path, hash, std_misc)]

#![allow(dead_code)]

extern crate alloc;
extern crate bytes;
extern crate nix;
extern crate time;

#[macro_use]
extern crate log;
#[macro_use]
extern crate bitflags;

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
    IoHandle,
    IoDesc,
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
    EventLoopError
};
pub use timer::{
    Timeout,
    TimerError,
    TimerResult
};
pub use os::token::{
    Token,
};

pub use os::event;

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

// Re-export bytes
pub mod buf {
    pub use bytes::{
        Buf,
        MutBuf,
        ByteBuf,
        MutByteBuf,
        RingBuf,
        RingBufReader,
        RingBufWriter,
        SliceBuf,
        MutSliceBuf,
    };
}
