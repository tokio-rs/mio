use std::io;

use crate::{Interests, PollOpt, Ready, Registry, Token};
use crate::event::Event;

/// A value that may be registered with `Registry`
///
/// Values that implement `Evented` can be registered with `Registry`. Users of
/// Mio should not use the `Evented` trait functions directly. Instead, the
/// equivalent functions on `Registry` should be used.
///
/// See [`Registry`] for more details.
///
/// # Implementing `Evented`
///
/// There are two types of `Evented` values.
///
/// * **System** handles, which are backed by sockets or other system handles.
/// These `Evented` handles will be monitored by the system selector. In this
/// case, an implementation of `Evented` delegates to a lower level handle.
///
/// * **User** handles, which are driven entirely in user space using
/// [`Registration`] and [`SetReadiness`]. In this case, the implementer takes
/// responsibility for driving the readiness state changes.
///
/// [`Registry`]: ../struct.Registry.html
/// [`Registration`]: ../struct.Registration.html
/// [`SetReadiness`]: ../struct.SetReadiness.html
///
/// # Examples
///
/// Implementing `Evented` on a struct containing a socket:
///
/// ```
/// use mio::{Interests, Registry, PollOpt, Token};
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
///     fn register(&self, registry: &Registry, token: Token, interests: Interests, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `register` call to `socket`
///         self.socket.register(registry, token, interests, opts)
///     }
///
///     fn reregister(&self, registry: &Registry, token: Token, interests: Interests, opts: PollOpt)
///         -> io::Result<()>
///     {
///         // Delegate the `reregister` call to `socket`
///         self.socket.reregister(registry, token, interests, opts)
///     }
///
///     fn deregister(&self, registry: &Registry) -> io::Result<()> {
///         // Delegate the `deregister` call to `socket`
///         self.socket.deregister(registry)
///     }
/// }
/// ```
///
/// Implement `Evented` using [`Registration`] and [`SetReadiness`].
///
/// ```
/// use mio::{Ready, Interests, Registration, Registry, PollOpt, Token};
/// use mio::event::Evented;
///
/// use std::io;
/// use std::time::Instant;
/// use std::thread;
///
/// pub struct Deadline {
///     when: Instant,
///     registration: Registration,
/// }
///
/// impl Deadline {
///     pub fn new(when: Instant) -> Deadline {
///         let (registration, set_readiness) = Registration::new();
///
///         thread::spawn(move || {
///             let now = Instant::now();
///
///             if now < when {
///                 thread::sleep(when - now);
///             }
///
///             set_readiness.set_readiness(Ready::READABLE);
///         });
///
///         Deadline {
///             when: when,
///             registration: registration,
///         }
///     }
///
///     pub fn is_elapsed(&self) -> bool {
///         Instant::now() >= self.when
///     }
/// }
///
/// impl Evented for Deadline {
///     fn register(&self, registry: &Registry, token: Token, interests: Interests, opts: PollOpt)
///         -> io::Result<()>
///     {
///         self.registration.register(registry, token, interests, opts)
///     }
///
///     fn reregister(&self, registry: &Registry, token: Token, interests: Interests, opts: PollOpt)
///         -> io::Result<()>
///     {
///         self.registration.reregister(registry, token, interests, opts)
///     }
///
///     fn deregister(&self, registry: &Registry) -> io::Result<()> {
///         self.registration.deregister(registry)
///     }
/// }
/// ```
pub trait Evented {
    /// Register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use [`Registry::register`]
    /// instead. Implementors should handle registration by either delegating
    /// the call to another `Evented` type or creating a [`Registration`].
    ///
    /// [`Registry::register`]: ../struct.Registry.html#method.register
    /// [`Registration`]: ../struct.Registration.html
    fn register(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()>;

    /// Re-register `self` with the given `Registry` instance.
    ///
    /// This function should not be called directly. Use [`Registry::reregister`]
    /// instead. Implementors should handle re-registration by either delegating
    /// the call to another `Evented` type or calling
    /// [`SetReadiness::set_readiness`].
    ///
    /// [`Registry::reregister`]: ../struct.Registry.html#method.reregister
    /// [`SetReadiness::set_readiness`]: ../struct.SetReadiness.html#method.set_readiness
    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()>;

    /// Deregister `self` from the given `Registry` instance
    ///
    /// This function should not be called directly. Use [`Registry::deregister`]
    /// instead. Implementors should handle deregistration by either delegating
    /// the call to another `Evented` type or by dropping the [`Registration`]
    /// associated with `self`.
    ///
    /// [`Registry::deregister`]: ../struct.Registry.html#method.deregister
    /// [`Registration`]: ../struct.Registration.html
    fn deregister(&self, registry: &Registry) -> io::Result<()>;
}

impl Evented for Box<dyn Evented> {
    fn register(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().register(registry, token, interests, opts)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests, opts)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

impl<T: Evented> Evented for Box<T> {
    fn register(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().register(registry, token, interests, opts)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests, opts)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

impl<T: Evented> Evented for ::std::sync::Arc<T> {
    fn register(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().register(registry, token, interests, opts)
    }

    fn reregister(
        &self,
        registry: &Registry,
        token: Token,
        interests: Interests,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.as_ref().reregister(registry, token, interests, opts)
    }

    fn deregister(&self, registry: &Registry) -> io::Result<()> {
        self.as_ref().deregister(registry)
    }
}

/*
 *
 * ===== Mio internal helpers =====
 *
 */

pub fn ready_as_usize(events: Ready) -> usize {
    events.as_usize()
}

pub fn opt_as_usize(opt: PollOpt) -> usize {
    opt.as_usize()
}

pub fn ready_from_usize(events: usize) -> Ready {
    Ready::from_usize(events)
}

pub fn opt_from_usize(opt: usize) -> PollOpt {
    PollOpt::from_usize(opt)
}

// Used internally to mutate an `Event` in place
// Not used on all platforms
#[allow(dead_code)]
pub fn kind_mut(event: &mut Event) -> &mut Ready {
    event.readiness_mut()
}
