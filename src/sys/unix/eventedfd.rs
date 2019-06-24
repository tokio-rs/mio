use crate::event::Evented;
use crate::{poll, Interests, Registry, Token};

use std::io;
use std::os::unix::io::RawFd;

/*
 *
 * ===== EventedFd =====
 *
 */

#[derive(Debug)]

/// Adapter for [`RawFd`] providing an [`Evented`] implementation.
///
/// `EventedFd` enables registering any type with an FD with [`Poll`].
///
/// While only implementations for TCP and UDP are provided, Mio supports
/// registering any FD that can be registered with the underlying OS selector.
/// `EventedFd` provides the necessary bridge.
///
/// Note that `EventedFd` takes a `&RawFd`. This is because `EventedFd` **does
/// not** take ownership of the FD. Specifically, it will not manage any
/// lifecycle related operations, such as closing the FD on drop. It is expected
/// that the `EventedFd` is constructed right before a call to
/// [`Poll::register`]. See the examples for more detail.
///
/// # Examples
///
/// Basic usage
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// use mio::{Interests, Poll, Token};
/// use mio::unix::EventedFd;
///
/// use std::os::unix::io::AsRawFd;
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
///     &EventedFd(&listener.as_raw_fd()),
///     Token(0),
///     Interests::READABLE)?;
/// #     Ok(())
/// # }
/// ```
///
/// Implementing [`Evented`] for a custom type backed by a [`RawFd`].
///
/// ```
/// use mio::{Interests, Registry, Token};
/// use mio::event::Evented;
/// use mio::unix::EventedFd;
///
/// use std::os::unix::io::RawFd;
/// use std::io;
///
/// # #[allow(dead_code)]
/// pub struct MyIo {
///     fd: RawFd,
/// }
///
/// impl Evented for MyIo {
///     fn register(&self, registry: &Registry, token: Token, interests: Interests)
///         -> io::Result<()>
///     {
///         EventedFd(&self.fd).register(registry, token, interests)
///     }
///
///     fn reregister(&self, registry: &Registry, token: Token, interests: Interests)
///         -> io::Result<()>
///     {
///         EventedFd(&self.fd).reregister(registry, token, interests)
///     }
///
///     fn deregister(&self, registry: &Registry) -> io::Result<()> {
///         EventedFd(&self.fd).deregister(registry)
///     }
/// }
/// ```
///
/// [`RawFd`]: https://doc.rust-lang.org/std/os/unix/io/type.RawFd.html
/// [`Evented`]: ../event/trait.Evented.html
/// [`Poll`]: ../struct.Poll.html
/// [`Poll::register`]: ../struct.Poll.html#method.register
pub struct EventedFd<'a>(pub &'a RawFd);

impl<'a> Evented for EventedFd<'a> {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        poll::selector(registry).register(*self.0, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        poll::selector(registry).reregister(*self.0, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        poll::selector(registry).deregister(*self.0)
    }
}
