use crate::{Interest, Registry, Token};

use std::io;
use std::ops::DerefMut;

/// An event source that may be registered with [`Registry`].
///
/// Types that implement `event::Source` can be registered with
/// `Registry`. Users of Mio **should not** use the `event::Source` trait
/// functions directly. Instead, the equivalent functions on `Registry` should
/// be used.
///
/// See [`Registry`] for more details.
///
/// [`Registry`]: crate::Registry
///
/// # Implementing `event::Source`
///
/// Event sources are always backed by system handles, such as sockets or other
/// system handles. These `event::Source`s will be monitored by the system
/// selector. An implementation of `Source` will almost always delegates to a
/// lower level handle. Examples of this are [`TcpStream`]s, the *unix only*
/// [`SourceFd`] or the *windows only* counterpart [`SourceSocket`].
///
/// [`TcpStream`]: crate::net::TcpStream
/// [`SourceFd`]: crate::unix::SourceFd
/// [`SourceSocket`]: crate::windows::SourceSocket
///
/// # Dropping `event::Source`s
///
/// All `event::Source`s, unless otherwise specified, need to be [deregistered]
/// before being dropped for them to not leak resources. This goes against the
/// normal drop behaviour of types in Rust which cleanup after themselves, e.g.
/// a `File` will close itself. However since deregistering needs access to
/// [`Registry`] this cannot be done while being dropped.
///
/// [deregistered]: crate::Registry::deregister
///
/// # Examples
///
/// Implementing `Source` on a struct containing a socket:
///
/// ```
/// use mio::{Interest, Registry, Token};
/// use mio::event::Source;
/// use mio::net::TcpStream;
///
/// use std::io;
///
/// # #[allow(dead_code)]
/// pub struct MySource {
///     socket: TcpStream,
/// }
///
/// impl Source for MySource {
///     fn register(&mut self, registry: &Registry, token: Token, interests: Interest)
///         -> io::Result<()>
///     {
///         // Delegate the `register` call to `socket`
///         self.socket.register(registry, token, interests)
///     }
///
///     fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest)
///         -> io::Result<()>
///     {
///         // Delegate the `reregister` call to `socket`
///         self.socket.reregister(registry, token, interests)
///     }
///
///     fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
///         // Delegate the `deregister` call to `socket`
///         self.socket.deregister(registry)
///     }
/// }
/// ```
pub trait Source {
    /// Register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use [`Registry::register`]
    /// instead. Implementors should handle registration by delegating the call
    /// to another `Source` type.
    ///
    /// [`Registry::register`]: crate::Registry::register
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()>;

    /// Re-register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use
    /// [`Registry::reregister`] instead. Implementors should handle
    /// re-registration by either delegating the call to another `Source` type.
    ///
    /// [`Registry::reregister`]: crate::Registry::reregister
    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()>;

    /// Deregister `self` from the given `Registry` instance.
    ///
    /// This function should not be called directly. Use
    /// [`Registry::deregister`] instead. Implementors should handle
    /// deregistration by delegating the call to another `Source` type.
    ///
    /// [`Registry::deregister`]: crate::Registry::deregister
    fn deregister(&mut self, registry: &Registry) -> io::Result<()>;
}

impl<T, S> Source for T
where
    T: DerefMut<Target = S>,
    S: Source + ?Sized,
{
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.deref_mut().register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.deref_mut().reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        self.deref_mut().deregister(registry)
    }
}
