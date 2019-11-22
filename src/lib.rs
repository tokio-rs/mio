#![doc(html_root_url = "https://docs.rs/mio/0.7.0")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
// Disallow warnings when running tests.
#![cfg_attr(test, deny(warnings))]
// Disallow warnings in examples.
#![doc(test(attr(deny(warnings))))]

//! Mio is a fast, low-level I/O library for Rust focusing on non-blocking APIs
//! and event notification for building high performance I/O apps with as little
//! overhead as possible over the OS abstractions.
//!
//! # Usage
//!
//! Using Mio starts by creating a [`Poll`], which reads events from the OS and
//! puts them into [`Events`]. You can handle I/O events from the OS with it.
//!
//! For more detail, see [`Poll`].
//!
//! [`Poll`]: struct.Poll.html
//! [`Events`]: struct.Events.html
//!
//! ## Examples
//!
//! Examples can found in the `examples` directory of the source code, or [on
//! GitHub].
//!
//! [on GitHub]: https://github.com/tokio-rs/mio/tree/master/examples
//!
//! ## Guide
//!
//! A getting started guide is available in the
#![cfg_attr(feature = "guide", doc = "[`guide`](crate::guide) module.")]
#![cfg_attr(
    not(feature = "guide"),
    doc = "`guide` (only available when the `guide` feature is enabled)."
)]

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

// Enable with `cargo doc --features guide`.
#[cfg(feature = "guide")]
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
