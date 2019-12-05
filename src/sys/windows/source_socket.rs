use super::InternalState;
use crate::{event, poll, Interest, Registry, Token};

use std::io;
use std::os::windows::io::RawSocket;

/// Adapter for [`RawSocket`] providing an [`event::Source`] implementation.
///
/// `SourceSocket` enables registering any type with an socket with [`Poll`].
///
/// While only implementations for TCP and UDP are provided, Mio supports
/// registering any socket that can be registered with the underlying OS
/// selector. `SourceSocket` provides the necessary bridge.
///
/// Each `SourceSocket` is tied to a [`SocketState`], which must be manually
/// managed by the user. In each call to register the same `SocketState` much be
/// provided.
///
/// Furthermore **all** I/O operations (such as `read`, `write`, etc.) must go
/// through [`SocketState::do_io`] to ensure the `SocketState` is updated
/// accordingly.
///
/// [`event::Source`]: crate::event::Source
/// [`Poll`]: crate::Poll
///
/// # Notes
///
/// `SourceSocket` takes a `&RawSocket`. This is because `SourceSocket` **does
/// not** take ownership of the socket. Specifically, it will not manage any
/// lifecycle related operations, such as closing the socket on drop. It is
/// expected that the `SourceSocket` is constructed right before a call to
/// [`Registry::register`]. See the examples for more detail.
///
/// What happens when when `SourceSocket` is used with a different `SocketState`
/// then previously is undefined.
///
/// [`Registry::register`]: crate::Registry::register
///
/// # Examples
///
/// Basic usage.
///
/// ```
/// # use std::error::Error;
/// # fn main() -> Result<(), Box<dyn Error>> {
/// use mio::windows::{SourceSocket, SocketState};
/// use mio::{Interest, Poll, Token};
///
/// use std::net::TcpListener;
/// use std::os::windows::io::AsRawSocket;
///
/// // Bind a std listener
/// let listener = TcpListener::bind("127.0.0.1:0")?;
/// let mut listener_state = SocketState::new();
///
/// let poll = Poll::new()?;
///
/// // Register the listener
/// poll.registry().register(
///     &mut SourceSocket(&listener.as_raw_socket(), &mut listener_state),
///     Token(0),
///     Interest::READABLE)?;
/// #     Ok(())
/// # }
/// ```
///
/// Implementing [`event::Source`] for a custom type backed by a [`RawSocket`].
///
/// ```
/// use mio::windows::{SourceSocket, SocketState};
/// use mio::{event, Interest, Registry, Token};
///
/// use std::io;
/// use std::os::windows::io::RawSocket;
///
/// # #[allow(dead_code)]
/// pub struct MyIo {
///     socket: RawSocket,
///     state: SocketState,
/// }
///
/// impl event::Source for MyIo {
///     fn register(&mut self, registry: &Registry, token: Token, interests: Interest)
///         -> io::Result<()>
///     {
///         SourceSocket(&self.socket, &mut self.state).register(registry, token, interests)
///     }
///
///     fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest)
///         -> io::Result<()>
///     {
///         SourceSocket(&self.socket, &mut self.state).reregister(registry, token, interests)
///     }
///
///     fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
///         SourceSocket(&self.socket, &mut self.state).deregister(registry)
///     }
/// }
/// ```
#[derive(Debug)]
pub struct SourceSocket<'a>(pub &'a RawSocket, pub &'a mut SocketState);

/// The state of a socket. Used in [`SourceSocket`], see it for more details.
#[derive(Debug)]
pub struct SocketState {
    // This is `None` if the socket has not yet been registered.
    state: Option<Box<InternalState>>,
}

impl SocketState {
    /// Create a new `SocketState`.
    pub fn new() -> SocketState {
        SocketState { state: None }
    }

    /// Execute an I/O operations ensuring that the socket is re-registered if
    /// it hit a [`WouldBlock`] error.
    ///
    /// # Notes
    ///
    /// This method is required to be called for **all** I/O operations to
    /// ensure the user will receive events once the socket is ready again after
    /// returning a [`WouldBlock`] error.
    ///
    /// [`WouldBlock`]: io::ErrorKind::WouldBlock
    pub fn do_io<F, T>(&self, f: F) -> io::Result<T>
    where
        F: FnOnce() -> io::Result<T>,
    {
        let result = f();
        if let Err(ref e) = result {
            if e.kind() == io::ErrorKind::WouldBlock {
                self.state.as_ref().map_or(Ok(()), |state| {
                    state
                        .selector
                        .reregister(&state.sock_state, state.token, state.interests)
                })?;
            }
        }
        result
    }
}

impl<'a> event::Source for SourceSocket<'a> {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        if self.1.state.is_some() {
            Err(io::Error::from(io::ErrorKind::AlreadyExists))
        } else {
            poll::selector(registry)
                .register(*self.0, token, interests)
                .map(|state| {
                    self.1.state = Some(Box::new(state));
                })
        }
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        match self.1.state.as_mut() {
            Some(state) => poll::selector(registry)
                .reregister(&state.sock_state, token, interests)
                .map(|()| {
                    state.token = token;
                    state.interests = interests;
                }),
            None => Err(io::Error::from(io::ErrorKind::NotFound)),
        }
    }

    fn deregister(&mut self, _registry: &Registry) -> io::Result<()> {
        match self.1.state.as_mut() {
            Some(state) => {
                {
                    let mut sock_state = state.sock_state.lock().unwrap();
                    sock_state.mark_delete();
                }
                self.1.state = None;
                Ok(())
            }
            None => Err(io::Error::from(io::ErrorKind::NotFound)),
        }
    }
}
