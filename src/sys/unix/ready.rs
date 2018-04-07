use event_imp::Ready;

use std::{mem, ops};

bitflags! {
    /// Unix specific extensions to `Ready`
    ///
    /// Provides additional readiness event kinds that are available on unix
    /// platforms. Unix platforms are able to provide readiness events for
    /// additional socket events, such as HUP and error.
    ///
    /// HUP events occur when the remote end of a socket hangs up. In the TCP case,
    /// this occurs when the remote end of a TCP socket shuts down writes.
    ///
    /// Error events occur when the socket enters an error state. In this case, the
    /// socket will also receive a readable or writable event. Reading or writing to
    /// the socket will result in an error.
    ///
    /// Conversion traits are implemented between `Ready` and `UnixReady`. See the
    /// examples.
    ///
    /// For high level documentation on polling and readiness, see [`Poll`].
    ///
    /// # Examples
    ///
    /// Most of the time, all that is needed is using bit operations
    ///
    /// ```
    /// use mio::Ready;
    /// use mio::unix::UnixReady;
    ///
    /// let ready = Ready::readable() | UnixReady::hup();
    ///
    /// assert!(ready.is_readable());
    /// assert!(UnixReady::from(ready).is_hup());
    /// ```
    ///
    /// Basic conversion between ready types.
    ///
    /// ```
    /// use mio::Ready;
    /// use mio::unix::UnixReady;
    ///
    /// // Start with a portable ready
    /// let ready = Ready::readable();
    ///
    /// // Convert to a unix ready, adding HUP
    /// let mut unix_ready = UnixReady::from(ready) | UnixReady::hup();
    ///
    /// unix_ready.insert(UnixReady::error());
    ///
    /// // `unix_ready` maintains readable interest
    /// assert!(unix_ready.is_readable());
    /// assert!(unix_ready.is_hup());
    /// assert!(unix_ready.is_error());
    ///
    /// // Convert back to `Ready`
    /// let ready = Ready::from(unix_ready);
    ///
    /// // Readable is maintained
    /// assert!(ready.is_readable());
    /// ```
    ///
    /// Registering readable and error interest on a socket
    ///
    /// ```
    /// # use std::error::Error;
    /// # fn try_main() -> Result<(), Box<Error>> {
    /// use mio::{Ready, Poll, PollOpt, Token};
    /// use mio::net::TcpStream;
    /// use mio::unix::UnixReady;
    ///
    /// let addr = "216.58.193.68:80".parse()?;
    /// let socket = TcpStream::connect(&addr)?;
    ///
    /// let mut poll = Poll::new()?;
    ///
    /// poll.register()
    ///     .register(&socket,
    ///               Token(0),
    ///               Ready::readable() | UnixReady::error(),
    ///               PollOpt::EDGE)?;
    /// #     Ok(())
    /// # }
    /// #
    /// # fn main() {
    /// #     try_main().unwrap();
    /// # }
    /// ```
    ///
    /// [`Poll`]: ../struct.Poll.html
    /// [readiness]: struct.Poll.html#readiness-operations
    pub struct UnixReady: usize {
        /// `Ready` representing error readiness.
        ///
        /// **Note that only readable and writable readiness is guaranteed to be
        /// supported on all platforms**. This means that `error` readiness
        /// should be treated as a hint. For more details, see [readiness] in the
        /// poll documentation.
        ///
        /// See [`Poll`] for more documentation on polling.
        ///
        /// # Examples
        ///
        /// ```
        /// use mio::unix::UnixReady;
        ///
        /// let ready = UnixReady::error();
        ///
        /// assert!(ready.is_error());
        /// ```
        ///
        /// [`Poll`]: ../struct.Poll.html
        /// [readiness]: ../struct.Poll.html#readiness-operations
        const ERROR = 0b00100;
        /// `Ready` representing HUP readiness.
        ///
        /// A HUP (or hang-up) signifies that a stream socket **peer** closed the
        /// connection, or shut down the writing half of the connection.
        ///
        /// **Note that only readable and writable readiness is guaranteed to be
        /// supported on all platforms**. This means that `hup` readiness
        /// should be treated as a hint. For more details, see [readiness] in the
        /// poll documentation.
        ///
        /// See [`Poll`] for more documentation on polling.
        ///
        /// # Examples
        ///
        /// ```
        /// use mio::unix::UnixReady;
        ///
        /// let ready = UnixReady::hup();
        ///
        /// assert!(ready.is_hup());
        /// ```
        ///
        /// [`Poll`]: ../struct.Poll.html
        /// [readiness]: ../struct.Poll.html#readiness-operations
        const HUP   = 0b01000;
        #[cfg(any(target_os = "dragonfly",
        target_os = "freebsd", target_os = "ios", target_os = "macos"))]
        /// `Ready` representing AIO completion readiness
        ///
        /// See [`Poll`] for more documentation on polling.
        ///
        /// # Examples
        ///
        /// ```
        /// use mio::unix::UnixReady;
        ///
        /// let ready = UnixReady::aio();
        ///
        /// assert!(ready.is_aio());
        /// ```
        ///
        /// [`Poll`]: ../struct.Poll.html
        const AIO   = 0b010000;
        /// `Ready` representing LIO completion readiness
        ///
        /// See [`Poll`] for more documentation on polling.
        ///
        /// # Examples
        ///
        /// ```
        /// use mio::unix::UnixReady;
        ///
        /// let ready = UnixReady::lio();
        ///
        /// assert!(ready.is_lio());
        /// ```
        ///
        /// [`Poll`]: struct.Poll.html
        #[cfg(any(target_os = "dragonfly", target_os = "freebsd"))]
        const LIO   = 0b100000;
    }
}

impl UnixReady {
    #[deprecated(since = "0.7.0", note = "use UnixReady::AIO instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    #[cfg(any(target_os = "dragonfly",
        target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    pub fn aio() -> UnixReady {
        Self::AIO
    }

    #[cfg(not(any(target_os = "dragonfly",
        target_os = "freebsd", target_os = "ios", target_os = "macos")))]
    #[deprecated(since = "0.6.12", note = "this function is now platform specific")]
    #[doc(hidden)]
    pub fn aio() -> UnixReady {
        Self::empty()
    }

    #[deprecated(since = "0.7.0", note = "use UnixReady::ERROR instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn error() -> UnixReady {
        Self::ERROR
    }

    #[deprecated(since = "0.7.0", note = "use UnixReady::HUP instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn hup() -> UnixReady {
        Self::HUP
    }

    #[deprecated(since = "0.7.0", note = "use UnixReady::LIO instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    #[cfg(any(target_os = "dragonfly", target_os = "freebsd"))]
    pub fn lio() -> UnixReady {
        Self::LIO
    }

    /// Returns true if `Ready` contains AIO readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::unix::UnixReady;
    ///
    /// let ready = UnixReady::aio();
    ///
    /// assert!(ready.is_aio());
    /// ```
    ///
    /// [`Poll`]: ../struct.Poll.html
    #[inline]
    #[cfg(any(target_os = "dragonfly",
        target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    pub fn is_aio(&self) -> bool {
        self.contains(Self::AIO)
    }

    #[deprecated(since = "0.6.12", note = "this function is now platform specific")]
    #[cfg(feature = "with-deprecated")]
    #[cfg(not(any(target_os = "dragonfly",
        target_os = "freebsd", target_os = "ios", target_os = "macos")))]
    #[doc(hidden)]
    pub fn is_aio(&self) -> bool {
        false
    }

    /// Returns true if the value includes error readiness
    ///
    /// **Note that only readable and writable readiness is guaranteed to be
    /// supported on all platforms**. This means that `error` readiness should
    /// be treated as a hint. For more details, see [readiness] in the poll
    /// documentation.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::unix::UnixReady;
    ///
    /// let ready = UnixReady::error();
    ///
    /// assert!(ready.is_error());
    /// ```
    ///
    /// [`Poll`]: ../struct.Poll.html
    /// [readiness]: ../struct.Poll.html#readiness-operations
    #[inline]
    pub fn is_error(&self) -> bool {
        self.contains(Self::ERROR)
    }

    /// Returns true if the value includes HUP readiness
    ///
    /// A HUP (or hang-up) signifies that a stream socket **peer** closed the
    /// connection, or shut down the writing half of the connection.
    ///
    /// **Note that only readable and writable readiness is guaranteed to be
    /// supported on all platforms**. This means that `hup` readiness
    /// should be treated as a hint. For more details, see [readiness] in the
    /// poll documentation.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::unix::UnixReady;
    ///
    /// let ready = UnixReady::hup();
    ///
    /// assert!(ready.is_hup());
    /// ```
    ///
    /// [`Poll`]: ../struct.Poll.html
    /// [readiness]: ../struct.Poll.html#readiness-operations
    #[inline]
    pub fn is_hup(&self) -> bool {
        self.contains(Self::HUP)
    }

    /// Returns true if `Ready` contains LIO readiness
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// # Examples
    ///
    /// ```
    /// use mio::unix::UnixReady;
    ///
    /// let ready = UnixReady::lio();
    ///
    /// assert!(ready.is_lio());
    /// ```
    #[inline]
    #[cfg(any(target_os = "dragonfly", target_os = "freebsd"))]
    pub fn is_lio(&self) -> bool {
        self.contains(Self::LIO)
    }
}

impl From<Ready> for UnixReady {
    fn from(src: Ready) -> UnixReady {
        UnixReady { bits: src.bits() }
    }
}

impl From<UnixReady> for Ready {
    fn from(src: UnixReady) -> Ready {
        Ready::new(src.bits)
    }
}

impl ops::Deref for UnixReady {
    type Target = Ready;

    fn deref(&self) -> &Ready {
        unsafe { mem::transmute(self) }
    }
}

impl ops::DerefMut for UnixReady {
    fn deref_mut(&mut self) -> &mut Ready {
        unsafe { mem::transmute(self) }
    }
}
