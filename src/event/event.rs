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
    /// Method is available on all platforms, but not all platforms trigger the
    /// error event.
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
    /// Method is available on all platforms, but not all platforms trigger the
    /// HUP event.
    ///
    /// Because of the above be cautions when using this in cross-platform
    /// applications, Mio makes no attempt at normalising this indicator and
    /// only provides a convenience method to read it. Refer to the selector
    /// documentation (below) when using this indicator.
    ///
    /// The table below shows what flags are checked on what OS.
    ///
    /// | [OS selector] | Flag(s) checked |
    /// |---------------|-----------------|
    /// | [epoll]       | `EPOLLHUP`      |
    /// | [kqueue]      | `EV_EOF`        |
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
    /// Method is available on all platforms, but not all platforms trigger the
    /// read HUP event.
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
    /// Method is available on all platforms, but not all platforms trigger the
    /// priority event.
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
    /// Method is available on all platforms, but not all platforms support AIO.
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
    /// Method is available on all platforms, but only FreeBSD supports LIO. On
    /// FreeBSD this method checks the `EVFILT_LIO` flag.
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
