//! # Notes
//!
//! The current implementation is somewhat limited. The `Waker` is not
//! implemented, as at the time of writing there is no way to support to wake-up
//! a thread from calling `poll_oneoff`.
//!
//! Furthermore the (re/de)register functions also don't work while concurrently
//! polling as both registering and polling requires a lock on the
//! `subscriptions`.
//!
//! Finally `Selector::try_clone`, required by `Registry::try_clone`, doesn't
//! work. However this could be implemented by use of an `Arc`.
//!
//! In summary, this only (barely) works using a single thread.

use std::cmp::max;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::{Interest, Token};

cfg_net! {
    mod net;

    pub(crate) use net::{tcp, udp};
}

/// Unique id for use as `SelectorId`.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

pub(crate) struct Selector {
    #[cfg(debug_assertions)]
    id: usize,
    /// Subscriptions (reads events) we're interested in.
    subscriptions: Mutex<Vec<wasi::Subscription>>,
    #[cfg(debug_assertions)]
    has_waker: AtomicBool,
}

impl Selector {
    pub(crate) fn new() -> io::Result<Selector> {
        Ok(Selector {
            #[cfg(debug_assertions)]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            subscriptions: Mutex::new(Vec::new()),
            #[cfg(debug_assertions)]
            has_waker: AtomicBool::new(false),
        })
    }

    #[cfg(debug_assertions)]
    pub(crate) fn id(&self) -> usize {
        self.id
    }

    pub(crate) fn try_clone(&self) -> io::Result<Selector> {
        unimplemented!("cloning Registry is not supported on wasi")
    }

    pub(crate) fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();

        let mut subscriptions = self.subscriptions.lock().unwrap();

        // If we want to a use a timeout in the `wasi_poll_oneoff()` function
        // we need another subscription to the list.
        if let Some(timeout) = timeout {
            subscriptions.push(timeout_subscription(timeout));
        }

        // `poll_oneoff` needs the same number of events as subscriptions.
        let length = subscriptions.len();
        events.reserve(length);
        debug_assert!(events.capacity() >= length);
        let res = unsafe { wasi::poll_oneoff(subscriptions.as_ptr(), events.as_mut_ptr(), length) };

        // Remove the timeout subscription we possibly added above.
        if timeout.is_some() {
            let timeout_sub = subscriptions.pop();
            debug_assert_eq!(
                timeout_sub.unwrap().u.tag,
                wasi::EVENTTYPE_CLOCK,
                "failed to remove timeout subscription"
            );
        }

        drop(subscriptions); // Unlock.

        match res {
            Ok(n_events) => {
                // Safety: `poll_oneoff` intialises the `events` for us.
                unsafe { events.set_len(n_events) };

                // Remove the timeout event.
                if timeout.is_some() {
                    if let Some(index) = events.iter().position(is_timeout_event) {
                        events.swap_remove(index);
                    }
                }

                check_errors(&events)
            }
            Err(err) => Err(io_err(err.raw_error())),
        }
    }

    pub(crate) fn register(
        &self,
        fd: wasi::Fd,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        let mut subscriptions = self.subscriptions.lock().unwrap();

        if interests.is_writable() {
            let subscription = wasi::Subscription {
                userdata: token.0 as wasi::Userdata,
                u: wasi::SubscriptionU {
                    tag: wasi::EVENTTYPE_FD_WRITE,
                    u: wasi::SubscriptionUU {
                        fd_write: wasi::SubscriptionFdReadwrite {
                            file_descriptor: fd,
                        },
                    },
                },
            };
            subscriptions.push(subscription);
        }

        if interests.is_readable() {
            let subscription = wasi::Subscription {
                userdata: token.0 as wasi::Userdata,
                u: wasi::SubscriptionU {
                    tag: wasi::EVENTTYPE_FD_READ,
                    u: wasi::SubscriptionUU {
                        fd_read: wasi::SubscriptionFdReadwrite {
                            file_descriptor: fd,
                        },
                    },
                },
            };
            subscriptions.push(subscription);
        }

        Ok(())
    }

    pub(crate) fn reregister(
        &self,
        fd: wasi::Fd,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.deregister(fd)
            .and_then(|()| self.register(fd, token, interests))
    }

    pub(crate) fn deregister(&self, fd: wasi::Fd) -> io::Result<()> {
        let mut subscriptions = self.subscriptions.lock().unwrap();

        let predicate = |subscription: &wasi::Subscription| {
            // Safety: `subscription.u.tag` defines the type of the union in
            // `subscription.u.u`.
            match subscription.u.tag {
                wasi::EVENTTYPE_FD_WRITE => unsafe {
                    subscription.u.u.fd_write.file_descriptor == fd
                },
                wasi::EVENTTYPE_FD_READ => unsafe {
                    subscription.u.u.fd_read.file_descriptor == fd
                },
                _ => false,
            }
        };
        if let Some(index) = subscriptions.iter().position(predicate) {
            subscriptions.swap_remove(index);
            // We might have two subscriptions (read and write).
            if let Some(index) = subscriptions.iter().skip(index).position(predicate) {
                subscriptions.swap_remove(index);
            }
            Ok(())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn register_waker(&self) -> bool {
        self.has_waker.swap(true, Ordering::AcqRel)
    }
}

/// Token used to a add a timeout subscription, also used in removing it again.
const TIMEOUT_TOKEN: wasi::Userdata = wasi::Userdata::max_value();

/// Returns a `wasi::Subscription` for `timeout`.
fn timeout_subscription(timeout: Duration) -> wasi::Subscription {
    wasi::Subscription {
        userdata: TIMEOUT_TOKEN,
        u: wasi::SubscriptionU {
            tag: wasi::EVENTTYPE_CLOCK,
            u: wasi::SubscriptionUU {
                clock: wasi::SubscriptionClock {
                    id: wasi::CLOCKID_MONOTONIC,
                    // Timestamp is in nanoseconds.
                    timeout: max(wasi::Timestamp::max_value() as u128, timeout.as_nanos())
                        as wasi::Timestamp,
                    // Give the implementation another millisecond to coalesce
                    // events.
                    precision: Duration::from_millis(1).as_nanos() as wasi::Timestamp,
                    // Zero means the `timeout` is considered relative to the
                    // current time.
                    flags: 0,
                },
            },
        },
    }
}

fn is_timeout_event(event: &wasi::Event) -> bool {
    event.r#type == wasi::EVENTTYPE_CLOCK && event.userdata == TIMEOUT_TOKEN
}

/// Check all events for possible errors, it returns the first error found.
fn check_errors(events: &[Event]) -> io::Result<()> {
    for event in events {
        if event.error != wasi::ERRNO_SUCCESS {
            return Err(io_err(event.error));
        }
    }
    Ok(())
}

/// Convert `wasi::Errno` into an `io::Error`.
fn io_err(errno: wasi::Errno) -> io::Error {
    // TODO: check if this is valid.
    io::Error::from_raw_os_error(errno as i32)
}

pub(crate) type Events = Vec<Event>;

pub(crate) type Event = wasi::Event;

pub(crate) mod event {
    use std::fmt;

    use crate::sys::Event;
    use crate::Token;

    pub(crate) fn token(event: &Event) -> Token {
        Token(event.userdata as usize)
    }

    pub(crate) fn is_readable(event: &Event) -> bool {
        event.r#type == wasi::EVENTTYPE_FD_READ
    }

    pub(crate) fn is_writable(event: &Event) -> bool {
        event.r#type == wasi::EVENTTYPE_FD_WRITE
    }

    pub(crate) fn is_error(_: &Event) -> bool {
        // Not supported? It could be that `wasi::Event.error` could be used for
        // this, but the docs say `error that occurred while processing the
        // subscription request`, so it's checked in `Select::select` already.
        false
    }

    pub(crate) fn is_read_closed(event: &Event) -> bool {
        event.r#type == wasi::EVENTTYPE_FD_READ
            // Safety: checked the type of the union above.
            && (event.fd_readwrite.flags & wasi::EVENTRWFLAGS_FD_READWRITE_HANGUP) != 0
    }

    pub(crate) fn is_write_closed(event: &Event) -> bool {
        event.r#type == wasi::EVENTTYPE_FD_WRITE
            // Safety: checked the type of the union above.
            && (event.fd_readwrite.flags & wasi::EVENTRWFLAGS_FD_READWRITE_HANGUP) != 0
    }

    pub(crate) fn is_priority(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub(crate) fn is_aio(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub(crate) fn is_lio(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub(crate) fn debug_details(f: &mut fmt::Formatter<'_>, event: &Event) -> fmt::Result {
        debug_detail!(
            TypeDetails(wasi::Eventtype),
            PartialEq::eq,
            wasi::EVENTTYPE_CLOCK,
            wasi::EVENTTYPE_FD_READ,
            wasi::EVENTTYPE_FD_WRITE,
        );

        #[allow(clippy::trivially_copy_pass_by_ref)]
        fn check_flag(got: &wasi::Eventrwflags, want: &wasi::Eventrwflags) -> bool {
            (got & want) != 0
        }
        debug_detail!(
            EventrwflagsDetails(wasi::Eventrwflags),
            check_flag,
            wasi::EVENTRWFLAGS_FD_READWRITE_HANGUP,
        );

        struct EventFdReadwriteDetails(wasi::EventFdReadwrite);

        impl fmt::Debug for EventFdReadwriteDetails {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct("EventFdReadwrite")
                    .field("nbytes", &self.0.nbytes)
                    .field("flags", &self.0.flags)
                    .finish()
            }
        }

        f.debug_struct("Event")
            .field("userdata", &event.userdata)
            .field("error", &event.error)
            .field("type", &TypeDetails(event.r#type))
            .field("fd_readwrite", &EventFdReadwriteDetails(event.fd_readwrite))
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct Waker {}

impl Waker {
    pub(crate) fn new(_: &Selector, _: Token) -> io::Result<Waker> {
        Ok(Waker {})
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        Ok(())
    }
}

cfg_io_source! {
    pub(crate) struct IoSourceState;

    impl IoSourceState {
        pub(crate) fn new() -> IoSourceState {
            IoSourceState
        }

        pub(crate) fn do_io<T, F, R>(&self, f: F, io: &T) -> io::Result<R>
        where
            F: FnOnce(&T) -> io::Result<R>,
        {
            // We don't hold state, so we can just call the function and
            // return.
            f(io)
        }
    }
}