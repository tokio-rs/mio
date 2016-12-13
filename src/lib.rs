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
//!
//! let addr = "127.0.0.1:13265".parse().unwrap();
//!
//! // Setup the server socket
//! let server = TcpListener::bind(&addr).unwrap();
//!
//! // Create an poll instance
//! let poll = Poll::new().unwrap();
//!
//! // Start listening for incoming connections
//! poll.register(&server, SERVER, Ready::readable(),
//!               PollOpt::edge()).unwrap();
//!
//! // Setup the client socket
//! let sock = TcpStream::connect(&addr).unwrap();
//!
//! // Register the socket
//! poll.register(&sock, CLIENT, Ready::readable(),
//!               PollOpt::edge()).unwrap();
//!
//! // Create storage for events
//! let mut events = Events::with_capacity(1024);
//!
//! loop {
//!     poll.poll(&mut events, None).unwrap();
//!
//!     for event in events.iter() {
//!         match event.token() {
//!             SERVER => {
//!                 // Accept and drop the socket immediately, this will close
//!                 // the socket and notify the client of the EOF.
//!                 let _ = server.accept();
//!             }
//!             CLIENT => {
//!                 // The server just shuts down the socket, let's just exit
//!                 // from our event loop.
//!                 return;
//!             }
//!             _ => unreachable!(),
//!         }
//!     }
//! }
//!
//! ```

#![doc(html_root_url = "https://docs.rs/mio/0.6.1")]
#![crate_name = "mio"]
#![cfg_attr(unix, deny(warnings))]

extern crate lazycell;
extern crate net2;
extern crate slab;

#[cfg(unix)]
extern crate nix;

#[cfg(unix)]
extern crate libc;

#[cfg(windows)]
extern crate miow;

#[cfg(windows)]
extern crate winapi;

#[cfg(windows)]
extern crate kernel32;

#[macro_use]
extern crate log;

#[cfg(test)]
extern crate env_logger;

mod event;
mod io;
mod net;
mod poll;
mod sys;
mod token;

pub mod channel;
pub mod timer;

/// EventLoop and other deprecated types
pub mod deprecated;

pub use event::{
    PollOpt,
    Ready,
    Event,
};
pub use io::{
    Evented,
    would_block,
};
pub use net::{
    tcp,
    udp,
};
pub use poll::{
    Poll,
    Events,
    EventsIter,
    Registration,
    SetReadiness,
};
pub use token::{
    Token,
};
pub use sys::IoVec;

#[cfg(unix)]
pub mod unix {
    //! Unix only extensions

    pub use sys::{
        EventedFd,
    };
}

// Conversion utilities
mod convert {
    use std::time::Duration;

    const NANOS_PER_MILLI: u32 = 1_000_000;
    const MILLIS_PER_SEC: u64 = 1_000;

    /// Convert a `Duration` to milliseconds, rounding up and saturating at
    /// `u64::MAX`.
    ///
    /// The saturating is fine because `u64::MAX` milliseconds are still many
    /// million years.
    pub fn millis(duration: Duration) -> u64 {
        // Round up.
        let millis = (duration.subsec_nanos() + NANOS_PER_MILLI - 1) / NANOS_PER_MILLI;
        duration.as_secs().saturating_mul(MILLIS_PER_SEC).saturating_add(millis as u64)
    }
}
