use crate::{sys, Token};

use std::fmt;

/// A readiness event.
///
/// `Event` is a readiness state paired with a [`Token`]. It is returned by
/// [`Poll::poll`].
///
/// For more documentation on polling and events, see [`Poll`].
///
/// [`Poll::poll`]: crate::Poll::poll
/// [`Poll`]: crate::Poll
/// [`Token`]: crate::Token
#[repr(transparent)]
pub struct Event {
    inner: sys::Event,
}

impl Event {
    /// Returns the event's token.
    #[inline]
    pub fn token(&self) -> Token {
        sys::event::token(&self.inner)
    }

    /// Returns true if the event contains readable readiness.
    #[inline]
    pub fn is_readable(&self) -> bool {
        sys::event::is_readable(&self.inner)
    }

    /// Returns true if the event contains writable readiness.
    #[inline]
    pub fn is_writable(&self) -> bool {
        sys::event::is_writable(&self.inner)
    }

    /// Returns true if the event contains error readiness.
    ///
    /// Error events occur when the socket enters an error state. In this case,
    /// the socket will also receive a readable or writable event. Reading or
    /// writing to the socket will result in an error.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    ///
    /// The table below shows what flags are checked on what OS.
    ///
    /// | [OS selector] | Flag(s) checked |
    /// |---------------|-----------------|
    /// | [epoll]       | `EPOLLERR`      |
    /// | [kqueue]      | `EV_ERROR` and `EV_EOF` with `fflags` set to `0`. |
    ///
    /// [OS selector]: ../struct.Poll.html#implementation-notes
    /// [epoll]: http://man7.org/linux/man-pages/man7/epoll.7.html
    /// [kqueue]: https://www.freebsd.org/cgi/man.cgi?query=kqueue&sektion=2
    #[inline]
    pub fn is_error(&self) -> bool {
        sys::event::is_error(&self.inner)
    }

    /// Returns true if the event contains HUP readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    ///
    /// When HUP events are received is different per platform. For example on
    /// epoll platforms HUP will be received when a TCP socket reached EOF,
    /// however on kqueue platforms a HUP event will be received when `shutdown`
    /// is called on the connection (both when shutting down the reading
    /// **and/or** writing side). Meaning that even though this function might
    /// return true on one platform in a given situation, it might return false
    /// on a different platform in the same situation. Furthermore even if true
    /// was returned on both platforms it could simply mean something different
    /// on the two platforms.
    ///
    /// Because of the above be cautions when using this in cross-platform
    /// applications, Mio makes no attempt at normalising this indicator and
    /// only provides a convenience method to read it.
    ///
    /// The table below shows what flags are checked on what OS.
    ///
    /// | [OS selector] | Flag(s) checked |
    /// |---------------|-----------------|
    /// | [epoll]       | `EPOLLHUP`      |
    /// | [kqueue]      | Not supported   |
    ///
    /// [OS selector]: ../struct.Poll.html#implementation-notes
    /// [epoll]: http://man7.org/linux/man-pages/man7/epoll.7.html
    /// [kqueue]: https://www.freebsd.org/cgi/man.cgi?query=kqueue&sektion=2
    #[inline]
    pub fn is_hup(&self) -> bool {
        sys::event::is_hup(&self.inner)
    }

    /// Returns true if the event contains read HUP readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    ///
    /// Because of the above be cautions when using this in cross-platform
    /// applications, Mio makes no attempt at normalising this indicator and
    /// only provides a convenience method to read it. We advice looking at the
    /// documentation provided for the selectors (see below) when using this
    /// indicator.
    ///
    /// The table below shows what flags are checked on what OS.
    ///
    /// | [OS selector] | Flag(s) checked |
    /// |---------------|-----------------|
    /// | [epoll]       | `EPOLLRDHUP`    |
    /// | [kqueue]      | `EV_EOF`        |
    ///
    /// [OS selector]: ../struct.Poll.html#implementation-notes
    /// [epoll]: http://man7.org/linux/man-pages/man7/epoll.7.html
    /// [kqueue]: https://www.freebsd.org/cgi/man.cgi?query=kqueue&sektion=2
    #[inline]
    pub fn is_read_hup(&self) -> bool {
        sys::event::is_read_hup(&self.inner)
    }

    /// Returns true if the event contains priority readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    ///
    /// The table below shows what flags are checked on what OS.
    ///
    /// | [OS selector] | Flag(s) checked |
    /// |---------------|-----------------|
    /// | [epoll]       | `EPOLLPRI`      |
    /// | [kqueue]      | *Not supported* |
    ///
    /// [OS selector]: ../struct.Poll.html#implementation-notes
    /// [epoll]: http://man7.org/linux/man-pages/man7/epoll.7.html
    /// [kqueue]: https://www.freebsd.org/cgi/man.cgi?query=kqueue&sektion=2
    #[inline]
    pub fn is_priority(&self) -> bool {
        sys::event::is_priority(&self.inner)
    }

    /// Returns true if the event contains AIO readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    ///
    /// The table below shows what flags are checked on what OS.
    ///
    /// | [OS selector] | Flag(s) checked |
    /// |---------------|-----------------|
    /// | [epoll]       | *Not supported* |
    /// | [kqueue]<sup>1</sup> | `EVFILT_AIO` |
    ///
    /// 1: Only supported on DragonFly BSD, FreeBSD, iOS and macOS.
    ///
    /// [OS selector]: ../struct.Poll.html#implementation-notes
    /// [epoll]: http://man7.org/linux/man-pages/man7/epoll.7.html
    /// [kqueue]: https://www.freebsd.org/cgi/man.cgi?query=kqueue&sektion=2
    #[inline]
    pub fn is_aio(&self) -> bool {
        sys::event::is_aio(&self.inner)
    }

    /// Returns true if the event contains LIO readiness.
    ///
    /// # Notes
    ///
    /// Method is available on all platforms, but not all platforms (can) use
    /// this indicator.
    ///
    /// This is currently only supported on FreeBSD and checks the `EVFILT_LIO`
    /// flag.
    #[inline]
    pub fn is_lio(&self) -> bool {
        sys::event::is_lio(&self.inner)
    }

    /// Create a reference to an `Event` from a platform specific event.
    pub(crate) fn from_sys_event_ref(sys_event: &sys::Event) -> &Event {
        unsafe {
            // This is safe because the memory layout of `Event` is
            // the same as `sys::Event` due to the `repr(transparent)` attribute.
            &*(sys_event as *const sys::Event as *const Event)
        }
    }
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Event")
            .field("token", &self.token())
            .field("readable", &self.is_readable())
            .field("writable", &self.is_writable())
            .field("error", &self.is_error())
            .field("hup", &self.is_hup())
            .field("read_hup", &self.is_read_hup())
            .field("priority", &self.is_priority())
            .field("aio", &self.is_aio())
            .field("lio", &self.is_lio())
            .finish()
    }
}
