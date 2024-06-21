#[cfg(all(
    not(mio_unsupported_force_poll_poll),
    not(all(
        not(mio_unsupported_force_waker_pipe),
        any(
            target_os = "freebsd",
            target_os = "ios",
            target_os = "macos",
            target_os = "tvos",
            target_os = "visionos",
            target_os = "watchos",
        )
    )),
    not(any(
        target_os = "espidf",
        target_os = "haiku",
        target_os = "hermit",
        target_os = "nto",
        target_os = "solaris",
        target_os = "vita"
    )),
))]
mod fdbased {
    use std::io;
    use std::os::fd::AsRawFd;

    #[cfg(all(
        not(mio_unsupported_force_waker_pipe),
        any(target_os = "android", target_os = "fuchsia", target_os = "linux"),
    ))]
    use crate::sys::unix::waker::eventfd::Waker as WakerInternal;
    #[cfg(any(
        mio_unsupported_force_waker_pipe,
        target_os = "aix",
        target_os = "dragonfly",
        target_os = "illumos",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "redox",
    ))]
    use crate::sys::unix::waker::pipe::Waker as WakerInternal;
    use crate::sys::Selector;
    use crate::{Interest, Token};

    #[derive(Debug)]
    pub(crate) struct Waker {
        waker: WakerInternal,
    }

    impl Waker {
        pub(crate) fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
            let waker = WakerInternal::new()?;
            selector.register(waker.as_raw_fd(), token, Interest::READABLE)?;
            Ok(Waker { waker })
        }

        pub(crate) fn wake(&self) -> io::Result<()> {
            self.waker.wake()
        }
    }
}

#[cfg(all(
    not(mio_unsupported_force_poll_poll),
    not(all(
        not(mio_unsupported_force_waker_pipe),
        any(
            target_os = "freebsd",
            target_os = "ios",
            target_os = "macos",
            target_os = "tvos",
            target_os = "visionos",
            target_os = "watchos",
        )
    )),
    not(any(
        target_os = "espidf",
        target_os = "haiku",
        target_os = "hermit",
        target_os = "nto",
        target_os = "solaris",
        target_os = "vita"
    )),
))]
pub(crate) use self::fdbased::Waker;

#[cfg(all(
    not(mio_unsupported_force_waker_pipe),
    any(
        target_os = "android",
        target_os = "espidf",
        target_os = "fuchsia",
        target_os = "hermit",
        target_os = "linux",
    )
))]
mod eventfd;

#[cfg(all(
    not(mio_unsupported_force_waker_pipe),
    any(
        mio_unsupported_force_poll_poll,
        target_os = "espidf",
        target_os = "fuchsia",
        target_os = "hermit",
    )
))]
pub(crate) use self::eventfd::Waker as WakerInternal;

#[cfg(any(
    mio_unsupported_force_waker_pipe,
    target_os = "aix",
    target_os = "dragonfly",
    target_os = "haiku",
    target_os = "illumos",
    target_os = "netbsd",
    target_os = "nto",
    target_os = "openbsd",
    target_os = "redox",
    target_os = "solaris",
    target_os = "vita",
))]
mod pipe;

#[cfg(any(
    all(
        mio_unsupported_force_poll_poll,
        any(
            mio_unsupported_force_waker_pipe,
            target_os = "aix",
            target_os = "dragonfly",
            target_os = "illumos",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "redox",
        )
    ),
    target_os = "haiku",
    target_os = "nto",
    target_os = "solaris",
    target_os = "vita",
))]
pub(crate) use self::pipe::Waker as WakerInternal;

#[cfg(any(
    mio_unsupported_force_poll_poll,
    target_os = "espidf",
    target_os = "haiku",
    target_os = "hermit",
    target_os = "nto",
    target_os = "solaris",
    target_os = "vita",
))]
mod poll {
    use crate::sys::Selector;
    use crate::Token;
    use std::io;

    #[derive(Debug)]
    pub(crate) struct Waker {
        selector: Selector,
        token: Token,
    }

    impl Waker {
        pub(crate) fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
            Ok(Waker {
                selector: selector.try_clone()?,
                token,
            })
        }

        pub(crate) fn wake(&self) -> io::Result<()> {
            self.selector.wake(self.token)
        }
    }
}

#[cfg(any(
    mio_unsupported_force_poll_poll,
    target_os = "espidf",
    target_os = "haiku",
    target_os = "hermit",
    target_os = "nto",
    target_os = "solaris",
    target_os = "vita",
))]
pub(crate) use self::poll::Waker;
