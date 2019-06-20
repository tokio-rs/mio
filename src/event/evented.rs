use crate::{Interests, Registry, Token};
use std::io;

/// A value that may be registered with [`Registry`].
///
/// Handles that implement `Evented` can be registered with `Registry`. Users of
/// Mio **should not** use the `Evented` trait functions directly. Instead, the
/// equivalent functions on `Registry` should be used.
///
/// See [`Registry`] for more details.
///
/// [`Registry`]: crate::Registry
///
/// # Implementing `Evented`
///
/// `Evented` values are always backed by **system** handles, which are backed
/// by sockets or other system handles. These `Evented` handles will be
/// monitored by the system selector. An implementation of `Evented` will almost
/// always delegates to a lower level handle. Examples of this are
/// [`TcpStream`]s, or the *unix only* [`EventedFd`].
///
/// [`TcpStream`]: crate::net::TcpStream
/// [`EventedFd`]: crate::unix::EventedFd
///
/// # Dropping `Evented` types
///
/// All `Evented` types, unless otherwise specified, need to be [deregistered]
/// before being dropped for them to not leak resources. This goes against the
/// normal drop behaviour of types in Rust which cleanup after themselves, e.g.
/// a `File` will close itself. However since deregistering needs access to
/// [`Registry`] this cannot be done while being dropped.
///
/// [deregistered]: crate::Registry::deregister
///
/// # Examples
///
/// Implementing `Evented` on a struct containing a socket:
///
/// ```
/// use mio::{Interests, Registry, Token};
/// use mio::event::Evented;
/// use mio::net::TcpStream;
///
/// use std::io;
///
/// pub struct MyEvented {
///     socket: TcpStream,
/// }
///
/// impl Evented for MyEvented {
///     fn register(&self, registry: &Registry, token: Token, interests: Interests)
///         -> io::Result<()>
///     {
///         // Delegate the `register` call to `socket`
///         self.socket.register(registry, token, interests)
///     }
///
///     fn reregister(&self, registry: &Registry, token: Token, interests: Interests)
///         -> io::Result<()>
///     {
///         // Delegate the `reregister` call to `socket`
///         self.socket.reregister(registry, token, interests)
///     }
///
///     fn deregister(&self, registry: &Registry) -> io::Result<()> {
///         // Delegate the `deregister` call to `socket`
///         self.socket.deregister(registry)
///     }
/// }
/// ```
pub trait Evented {
    /// Register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use [`Registry::register`]
    /// instead. Implementors should handle registration by delegating
    /// the call to another `Evented` type.
    ///
    /// [`Registry::register`]: ../struct.Registry.html#method.register
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()>;

    /// Re-register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use [`Registry::reregister`]
    /// instead. Implementors should handle re-registration by either delegating
    /// the call to another `Evented` type or calling
    /// [`SetReadiness::set_readiness`].
    ///
    /// [`Registry::reregister`]: ../struct.Registry.html#method.reregister
    /// [`SetReadiness::set_readiness`]: ../struct.SetReadiness.html#method.set_readiness
    fn reregister(&self, registry: &Registry, token: Token, interests: Interests)
        -> io::Result<()>;

    /// Deregister `self` from the given `Registry` instance
    ///
    /// This function should not be called directly. Use [`Registry::deregister`]
    /// instead. Implementors should handle deregistration by delegating
    /// the call to another `Evented` type.
    ///
    /// [`Registry::deregister`]: ../struct.Registry.html#method.deregister
    fn deregister(&self, registry: &Registry) -> io::Result<()>;
}

impl Evented for Box<dyn Evented> {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        self.as_ref().register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

impl<T: Evented> Evented for Box<T> {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        self.as_ref().register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

impl<T: Evented> Evented for ::std::sync::Arc<T> {
    fn register(&self, registry: &Registry, token: Token, interests: Interests) -> io::Result<()> {
        self.as_ref().register(registry, token, interests)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}
