use {io, poll, Evented, Ready, Poll, PollOpt, Token};
use libc;
use magenta;
use magenta::HandleBase;
use sys::fuchsia::{DontDrop, poll_opts_to_wait_async, sys};
use std::mem;
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};

/// Properties of an `EventedFd`'s current registration
#[derive(Debug)]
pub(in sys::fuchsia) struct EventedFdRegistration {
    pub token: Token,
    pub handle: DontDrop<magenta::Handle>,
    pub rereg_signals: Option<(magenta::Signals, magenta::WaitAsyncOpts)>,
}

/// An event-ed file descriptor. The file descriptor is owned by this structure.
#[derive(Debug)]
pub(in sys::fuchsia) struct EventedFdInner {
    /// Properties of the current registration.
    pub registration: Mutex<Option<EventedFdRegistration>>,

    /// Owned file descriptor.
    pub fd: RawFd,

    /// Owned `mxio_t` ponter.
    pub mxio: *const sys::mxio_t,
}

impl EventedFdInner {
    pub fn rereg_for_level(&self, port: &magenta::Port) {
        let registration_opt = self.registration.lock().unwrap();
        if let Some(ref registration) = *registration_opt {
            if let Some((rereg_signals, rereg_opts)) = registration.rereg_signals {
                let _res =
                    registration
                        .handle.inner_ref()
                        .wait_async(port,
                                    registration.token.0 as u64,
                                    rereg_signals,
                                    rereg_opts);
            }
        }
    }
}

impl Drop for EventedFdInner {
    fn drop(&mut self) {
        unsafe {
            sys::__mxio_release(self.mxio);
            let _ = libc::close(self.fd);
        }
    }
}

// `EventedInner` must be manually declared `Send + Sync` because it contains a `RawFd` and a
// `*const sys::mxio_t`. These are only used to make thread-safe system calls, so accessing
// them is entirely thread-safe.
//
// Note: one minor exception to this are the calls to `libc::close` and `__mxio_release`, which
// happen on `Drop`. These accesses are safe because `drop` can only be called at most once from
// a single thread, and after it is called no other functions can be called on the `EventedFdInner`.
unsafe impl Sync for EventedFdInner {}
unsafe impl Send for EventedFdInner {}

#[derive(Clone, Debug)]
pub(in sys::fuchsia) struct EventedFd {
    pub inner: Arc<EventedFdInner>
}

impl EventedFd {
    pub unsafe fn new(fd: RawFd) -> Self {
        let mxio = sys::__mxio_fd_to_io(fd);
        assert!(mxio != ::std::ptr::null(), "FileDescriptor given to EventedFd must be valid.");

        EventedFd {
            inner: Arc::new(EventedFdInner {
                registration: Mutex::new(None),
                fd: fd,
                mxio: mxio,
            })
        }
    }
}

impl Evented for EventedFd {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()>
    {
        let epoll_events = ioevent_to_epoll(interest, opts);

        let (handle, raw_handle, signals) = unsafe {
            let mut raw_handle: sys::mx_handle_t = mem::uninitialized();
            let mut signals: sys::mx_signals_t = mem::uninitialized();
            sys::__mxio_wait_begin(self.inner.mxio, epoll_events, &mut raw_handle, &mut signals);

            // We don't have ownership of the handle, so we can't drop it
            let handle = DontDrop::new(magenta::Handle::from_raw(raw_handle));
            (handle, raw_handle, signals)
        };


        let needs_rereg = opts.is_level() && !opts.is_oneshot();

        {
            let mut registration_lock = self.inner.registration.lock().unwrap();
            if registration_lock.is_some() {
                panic!("Called register on an already registered file descriptor.");
            }
            *registration_lock = Some(EventedFdRegistration {
                token: token,
                handle: DontDrop::new(unsafe { magenta::Handle::from_raw(raw_handle) }),
                rereg_signals: if needs_rereg {
                    Some((signals, poll_opts_to_wait_async(opts)))
                } else {
                    None
                },
            })
        }

        let registered = poll::selector(poll)
            .register_fd(handle.inner_ref(), self, token, signals, opts);

        if registered.is_err() {
            let mut registration_lock = self.inner.registration.lock().unwrap();
            *registration_lock = None;
        }

        registered
    }

    fn reregister(&self,
                  poll: &Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt) -> io::Result<()>
    {
        self.deregister(poll)?;
        self.register(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        let mut registration_lock = self.inner.registration.lock().unwrap();
        let old_registration = registration_lock.take()
            .expect("Tried to deregister on unregistered handle.");

        poll::selector(poll)
            .deregister_fd(old_registration.handle.inner_ref(), old_registration.token)
    }
}

fn ioevent_to_epoll(interest: Ready, opts: PollOpt) -> u32 {
    use event_imp::ready_from_usize;
    const HUP: usize   = 0b01000;

    let mut kind = 0;

    if interest.is_readable() {
        kind |= libc::EPOLLIN;
    }

    if interest.is_writable() {
        kind |= libc::EPOLLOUT;
    }

    if interest.contains(ready_from_usize(HUP)) {
        kind |= libc::EPOLLRDHUP;
    }

    if opts.is_edge() {
        kind |= libc::EPOLLET;
    }

    if opts.is_oneshot() {
        kind |= libc::EPOLLONESHOT;
    }

    if opts.is_level() {
        kind &= !libc::EPOLLET;
    }

    kind as u32
}
