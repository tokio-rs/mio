#![doc(html_root_url = "https://docs.rs/mio/0.7.0")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
// Disallow warnings when running tests.
#![cfg_attr(test, deny(warnings))]
// Disallow warnings in examples.
#![doc(test(attr(deny(warnings))))]

//! A fast, low-level IO library for Rust focusing on non-blocking APIs, event
//! notification, and other useful utilities for building high performance IO
//! apps.
//!
//! # Features
//!
//! * Non-blocking TCP, UDP
//! * I/O event queue backed by epoll, kqueue, and IOCP
//! * Zero allocations at runtime
//! * Platform specific extensions
//!
//! # Non-goals
//!
//! The following are specifically omitted from Mio and are left to the user or
//! higher-level libraries.
//!
//! * File operations
//! * Thread pools / multi-threaded event loop
//! * Timers
//!
//! # Platforms
//!
//! Currently supported platforms:
//!
//! * Android
//! * DragonFly BSD
//! * FreeBSD
//! * Linux
//! * NetBSD
//! * OpenBSD
//! * Solaris
//! * Windows
//! * iOS
//! * macOS
//!
//! Mio can handle interfacing with each of the event systems of the
//! aforementioned platforms. The details of their implementation are further
//! discussed in [`Poll`].
//!
//! # Usage
//!
//! Using Mio starts by creating a [`Poll`], which reads events from the OS and
//! puts them into [`Events`]. You can handle IO events from the OS with it.
//!
//! For more detail, see [`Poll`].
//!
//! [`Poll`]: struct.Poll.html
//! [`Events`]: struct.Events.html
//!
//! # Example
//!
//! ```
//! use mio::*;
//! use mio::net::{TcpListener, TcpStream};
//!
//! // Setup some tokens to allow us to identify which event is
//! // for which socket.
//! const SERVER: Token = Token(0);
//! const CLIENT: Token = Token(1);
//!
//! let addr = "127.0.0.1:13265".parse().unwrap();
//!
//! // Setup the server socket
//! let server = TcpListener::bind(addr).unwrap();
//!
//! // Create a poll instance
//! let mut poll = Poll::new().unwrap();
//!
//! // Start listening for incoming connections
//! poll.registry().register(&server, SERVER, Interests::READABLE).unwrap();
//!
//! // Setup the client socket
//! let sock = TcpStream::connect(addr).unwrap();
//!
//! // Register the socket
//! poll.registry().register(&sock, CLIENT, Interests::READABLE).unwrap();
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

mod interests;
mod poll;
mod sys;
mod token;
mod waker;

pub mod event;
pub mod net;

#[doc(no_inline)]
pub use event::Events;
pub use interests::Interests;
pub use poll::{Poll, Registry};
pub use token::Token;
pub use waker::Waker;

#[cfg(unix)]
pub mod unix {
    //! Unix only extensions.
    pub use crate::sys::SocketAddr;
    pub use crate::sys::SourceFd;
}

pub mod guide {
    //! # Getting started guide.
    //!
    //! Using Mio starts by creating a [`Poll`] instance, which reads events from
    //! the OS and puts them into [`Events`].
    //!
    //! [`Poll`]: crate::Poll
    //! [`Events`]: crate::Events
    //!
    //! ```
    //! # use mio::{Poll, Events};
    //! # fn main() -> std::io::Result<()> {
    //! // `Poll` allows for polling of readiness events.
    //! let poll = Poll::new()?;
    //! // `Events` is collection of readiness `Event`s and can be filled by calling
    //! // `Poll::poll`.
    //! let events = Events::with_capacity(128);
    //! # drop((poll, events));
    //! # Ok(())
    //! # }
    //! ```
    //!
    //! Next an [event source] needs to be registered with [`Poll`] to receive
    //! events for it. We'll use a [`TcpListener`] for which we'll receive an event
    //! once a connection is ready to be accepted.
    //!
    //! [event source]: crate::event::Source
    //! [`TcpListener`]: crate::net::TcpListener
    //!
    //! ```
    //! # use mio::net::TcpListener;
    //! # use mio::{Poll, Token, Interests};
    //! # fn main() -> std::io::Result<()> {
    //! # let poll = Poll::new()?;
    //! # let address = "127.0.0.1:0".parse().unwrap();
    //! // Create a `TcpListener`, binding it to `address`.
    //! let listener = TcpListener::bind(address)?;
    //!
    //! // Next we register it with `Poll` to receive events for it. The `SERVER`
    //! // `Token` is used to determine that we received an event for the listener
    //! // later on.
    //! const SERVER: Token = Token(0);
    //! poll.registry().register(&listener, SERVER, Interests::READABLE)?;
    //! # Ok(())
    //! # }
    //! ```
    //!
    //! As the name “event source” suggests it is a source of events which can
    //! be polled using a `Poll` instance. Multiple event sources can be
    //! [registered] with `Poll`. After we've done this we can start polling
    //! these events. If we do this in a loop we've got ourselves an event loop.
    //!
    //! [registered]: crate::Registry::register
    //!
    //! ```
    //! # use std::io;
    //! # use std::time::Duration;
    //! # use mio::net::TcpListener;
    //! # use mio::{Poll, Token, Interests, Events};
    //! # fn main() -> io::Result<()> {
    //! # let mut poll = Poll::new()?;
    //! # let mut events = Events::with_capacity(128);
    //! # let address = "127.0.0.1:0".parse().unwrap();
    //! # let listener = TcpListener::bind(address)?;
    //! # const SERVER: Token = Token(0);
    //! # poll.registry().register(&listener, SERVER, Interests::READABLE)?;
    //! // Start our event loop.
    //! loop {
    //!     // Poll Mio for events, waiting at most 100 milliseconds.
    //!     poll.poll(&mut events, Some(Duration::from_millis(100)))?;
    //!
    //!     // Process each event.
    //!     for event in events.iter() {
    //!         // We can use the token we previously provided to `register` to
    //!         // determine for which type the event is.
    //!         match event.token() {
    //!             SERVER => loop {
    //!                 // One or more connections are ready, so we'll attempt to
    //!                 // accept them in a loop.
    //!                 match listener.accept() {
    //!                     Ok((connection, address)) => {
    //!                         println!("Got a connection from: {}", address);
    //! #                       drop(connection);
    //!                     },
    //!                     // A "would block error" is returned if the operation
    //!                     // is not ready, so we'll stop trying to accept
    //!                     // connections.
    //!                     Err(ref err) if would_block(err) => break,
    //!                     Err(err) => return Err(err),
    //!                 }
    //!             }
    //! #           _ => unreachable!(),
    //!         }
    //!     }
    //! #   return Ok(());
    //! }
    //!
    //! fn would_block(err: &io::Error) -> bool {
    //!     err.kind() == io::ErrorKind::WouldBlock
    //! }
    //! # }
    //! ```
}
