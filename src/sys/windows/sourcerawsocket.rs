use crate::{event, poll, Interests, Registry, Token};

use std::io;
use std::os::windows::io::{AsRawSocket, RawSocket};

/// Adapter for [`RawSocket`] providing an [`event::Source`] implementation.
///
/// `SourceRawSocket` enables registering any type with an RawSocket with [`Poll`].
///
/// Note that `SourceRawSocket` takes a `&RawSocket`. This is because `SourceRawSocket` **does
/// not** take ownership of the RawSocket. Specifically, it will not manage any
/// lifecycle related operations, such as closing the RawSocket on drop. It is expected
/// that the `SourceRawSocket` is constructed right before a call to
/// [`Registry::register`]. See the examples for more detail.
///
/// [`event::Source`]: crate::event::Source
/// [`Poll`]: crate::Poll
/// [`Registry::register`]: crate::Registry::register
///
/// # Examples
///
/// Basic usage.
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// use mio::{Interests, Poll, Token};
/// use mio::windows::SourceRawSocket;
///
/// use std::os::windows::io::AsRawSocket;
/// use std::net::TcpListener;
///
/// // Bind a std listener
/// let listener = TcpListener::bind("127.0.0.1:0")?;
///
/// let poll = Poll::new()?;
/// let registry = poll.registry().clone();
///
/// // Register the listener
/// registry.register(
///     &SourceRawSocket(&listener.as_raw_socket()),
///     Token(0),
///     Interests::READABLE)?;
/// #     Ok(())
/// # }
/// ```
///
/// Implementing [`event::Source`] for a custom type backed by a [`RawFd`].
///
/// ```
/// use mio::{event, Interests, Registry, Token};
/// use mio::windows::SourceRawSocket;
///
/// use std::os::windows::io::RawSocket;
/// use std::io;
///
/// # #[allow(dead_code)]
/// pub struct MyIo {
///     raw_socket: RawSocket,
/// }
///
/// impl event::Source for MyIo {
///     fn register(&self, registry: &Registry, token: Token, interests: Interests)
///         -> io::Result<()>
///     {
///         SourceRawSocket(&self.raw_socket).register(registry, token, interests)
///     }
///
///     fn reregister(&self, registry: &Registry, token: Token, interests: Interests)
///         -> io::Result<()>
///     {
///         SourceRawSocket(&self.raw_socket).reregister(registry, token, interests)
///     }
///
///     fn deregister(&self, registry: &Registry) -> io::Result<()> {
///         SourceRawSocket(&self.raw_socket).deregister(registry)
///     }
/// }
/// ```
#[derive(Debug)]
pub struct SourceRawSocket<'a>(pub &'a RawSocket);

impl<'a> event::Source for SourceRawSocket<'a> {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        poll::selector(registry).register(self, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        poll::selector(registry).reregister(self, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        poll::selector(registry).deregister(self)
    }
}

impl<'a> AsRawSocket for SourceRawSocket<'a> {
    fn as_raw_socket(&self) -> RawSocket {
        *self.0
    }
}
