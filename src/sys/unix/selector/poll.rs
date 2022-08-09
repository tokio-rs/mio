use crate::{Interest, Token};
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt::{Debug, Formatter};
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};
use std::{fmt, io};

/// Unique id for use as `SelectorId`.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[cfg(target_os = "espidf")]
type NotifyType = u64;

#[cfg(not(target_os = "espidf"))]
type NotifyType = u8;

/// Interface to poll.
#[derive(Debug)]
pub struct Selector {
    /// File descriptors to poll.
    fds: Mutex<Fds>,

    /// The file descriptor of the read half of the notify pipe. This is also stored as the first
    /// file descriptor in `fds.poll_fds`.
    notify_read: RawFd,
    /// The file descriptor of the write half of the notify pipe.
    ///
    /// Data is written to this to wake up the current instance of `wait`, which can occur when the
    /// user notifies it (in which case `notified` would have been set) or when an operation needs
    /// to occur (in which case `waiting_operations` would have been incremented).
    notify_write: RawFd,

    /// The number of operations (`add`, `modify` or `delete`) that are currently waiting on the
    /// mutex to become free. When this is nonzero, `wait` must be suspended until it reaches zero
    /// again.
    waiting_operations: AtomicUsize,
    /// The condition variable that gets notified when `waiting_operations` reaches zero or
    /// `notified` becomes true.
    ///
    /// This is used with the `fds` mutex.
    operations_complete: Condvar,

    /// This selectors id.
    #[cfg(debug_assertions)]
    id: usize,

    /// Whether this selector currently has an associated waker.
    #[cfg(debug_assertions)]
    has_waker: AtomicBool,
}

/// The file descriptors to poll in a `Poller`.
#[derive(Debug, Clone)]
struct Fds {
    /// The list of `pollfds` taken by poll.
    ///
    /// The first file descriptor is always present and is used to notify the poller. It is also
    /// stored in `notify_read`.
    poll_fds: Vec<PollFd>,
    /// The map of each file descriptor to data associated with it. This does not include the file
    /// descriptors `notify_read` or `notify_write`.
    fd_data: HashMap<RawFd, FdData>,
}

/// Transparent wrapper around `libc::pollfd`, used to support `Debug` derives without adding the
/// `extra_traits` feature of `libc`.
#[repr(transparent)]
#[derive(Clone)]
struct PollFd(libc::pollfd);

impl Debug for PollFd {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("pollfd")
            .field("fd", &self.0.fd)
            .field("events", &self.0.events)
            .field("revents", &self.0.revents)
            .finish()
    }
}

/// Data associated with a file descriptor in a poller.
#[derive(Debug, Clone)]
struct FdData {
    /// The index into `poll_fds` this file descriptor is.
    poll_fds_index: usize,
    /// The key of the `Event` associated with this file descriptor.
    token: Token,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let notify_fds = Self::create_notify_fds()?;

        Ok(Self {
            fds: Mutex::new(Fds {
                poll_fds: vec![PollFd(libc::pollfd {
                    fd: notify_fds[0],
                    events: libc::POLLRDNORM,
                    revents: 0,
                })],
                fd_data: HashMap::new(),
            }),
            notify_read: notify_fds[0],
            notify_write: notify_fds[1],
            waiting_operations: AtomicUsize::new(0),
            operations_complete: Condvar::new(),
            #[cfg(debug_assertions)]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            #[cfg(debug_assertions)]
            has_waker: AtomicBool::new(false),
        })
    }

    fn create_notify_fds() -> io::Result<[libc::c_int; 2]> {
        let mut notify_fd = [0, 0];

        // Note that the eventfd() implementation in ESP-IDF deviates from the specification in the following ways:
        // 1) The file descriptor is always in a non-blocking mode, as if EFD_NONBLOCK was passed as a flag;
        //    passing EFD_NONBLOCK or calling fcntl(.., F_GETFL/F_SETFL) on the eventfd() file descriptor is not supported
        // 2) It always returns the counter value, even if it is 0. This is contrary to the specification which mandates
        //    that it should instead fail with EAGAIN
        //
        // (1) is not a problem for us, as we want the eventfd() file descriptor to be in a non-blocking mode anyway
        // (2) is also not a problem, as long as we don't try to read the counter value in an endless loop when we detect being notified
        #[cfg(target_os = "espidf")]
        {
            extern "C" {
                fn eventfd(initval: libc::c_uint, flags: libc::c_int) -> libc::c_int;
            }

            let fd = unsafe { eventfd(0, 0) };
            if fd == -1 {
                // TODO: Switch back to syscall! once
                // https://github.com/rust-lang/libc/pull/2864 is published
                return Err(std::io::ErrorKind::Other.into());
            }

            notify_fd[0] = fd;
            notify_fd[1] = fd;
        }

        #[cfg(not(target_os = "espidf"))]
        {
            syscall!(pipe(notify_fd.as_mut_ptr()))?;

            // Put the reading side into non-blocking mode.
            let notify_read_flags = syscall!(fcntl(notify_fd[0], libc::F_GETFL))?;

            syscall!(fcntl(
                notify_fd[0],
                libc::F_SETFL,
                notify_read_flags | libc::O_NONBLOCK
            ))?;
        }

        log::trace!(
            "new: notify_read={}, notify_write={}",
            notify_fd[0],
            notify_fd[1]
        );

        Ok(notify_fd)
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        let mut fds = self.modify_fds(|fds| Ok(fds.clone()))?;

        let notify_fds = Self::create_notify_fds()?;

        fds.poll_fds[0] = PollFd(libc::pollfd {
            fd: notify_fds[0],
            events: libc::POLLRDNORM,
            revents: 0,
        });

        Ok(Self {
            fds: Mutex::new(fds),
            notify_read: notify_fds[0],
            notify_write: notify_fds[1],
            waiting_operations: AtomicUsize::new(0),
            operations_complete: Condvar::new(),
            #[cfg(debug_assertions)]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            #[cfg(debug_assertions)]
            has_waker: AtomicBool::new(false),
        })
    }

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        log::trace!(
            "select: notify_read={}, timeout={:?}",
            self.notify_read,
            timeout
        );

        let deadline = timeout.map(|t| Instant::now() + t);

        events.clear();

        let mut fds = self.fds.lock().unwrap();

        // Complete all current operations.
        loop {
            if self.waiting_operations.load(Ordering::SeqCst) == 0 {
                break;
            }

            fds = self.operations_complete.wait(fds).unwrap();
        }

        // Perform the poll.
        let num_events = poll(&mut fds.poll_fds, deadline)?;
        let notified = fds.poll_fds[0].0.revents != 0;
        let num_fd_events = if notified { num_events - 1 } else { num_events };
        log::trace!(
            "new events: notify_read={}, num={}",
            self.notify_read,
            num_events
        );
        log::trace!("fds = {:?}", fds);

        // Read all notifications.
        if notified {
            if self.notify_read != self.notify_write {
                // When using the `pipe` syscall, we have to read all accumulated notifications in the pipe.
                while syscall!(read(self.notify_read, &mut [0; 64] as *mut _ as *mut _, 64)).is_ok()
                {
                }
            } else {
                // When using the `eventfd` syscall, it is OK to read just once, so as to clear the counter.
                // In fact, reading in a loop will result in an endless loop on the ESP-IDF
                // which is not following the specification strictly.
                let _ = self.pop_notification();
            }
        }

        // Store the events if there were any.
        if num_fd_events > 0 {
            let fds = &mut *fds;

            events.reserve(num_fd_events);
            for fd_data in fds.fd_data.values_mut() {
                let PollFd(poll_fd) = &mut fds.poll_fds[fd_data.poll_fds_index];
                if poll_fd.revents != 0 {
                    // Store event
                    events.push(Event {
                        token: fd_data.token,
                        events: poll_fd.revents,
                    });

                    if events.len() == num_fd_events {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        if fd == self.notify_read || fd == self.notify_write {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }

        log::trace!(
            "register: notify_read={}, fd={}, token={:?}, interests={:?}",
            self.notify_read,
            fd,
            token,
            interests
        );

        self.modify_fds(|fds| {
            if fds.fd_data.contains_key(&fd) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "\
                    same file descriptor registered twice for polling \
                    (an old file descriptor might have been closed without deregistration)\
                    ",
                ));
            }

            let poll_fds_index = fds.poll_fds.len();
            fds.fd_data.insert(
                fd,
                FdData {
                    poll_fds_index,
                    token,
                },
            );

            fds.poll_fds.push(PollFd(libc::pollfd {
                fd,
                events: interests_to_poll(interests),
                revents: 0,
            }));

            Ok(())
        })
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        log::trace!(
            "reregister: notify_read={}, fd={}, token={:?}, interests={:?}",
            self.notify_read,
            fd,
            token,
            interests
        );

        self.modify_fds(|fds| {
            let data = fds.fd_data.get_mut(&fd).ok_or(io::ErrorKind::NotFound)?;
            data.token = token;
            let poll_fds_index = data.poll_fds_index;
            fds.poll_fds[poll_fds_index].0.events = interests_to_poll(interests);

            Ok(())
        })
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        log::trace!("deregister: notify_read={}, fd={}", self.notify_read, fd);

        self.modify_fds(|fds| {
            let data = fds.fd_data.remove(&fd).ok_or(io::ErrorKind::NotFound)?;
            fds.poll_fds.swap_remove(data.poll_fds_index);
            if let Some(swapped_pollfd) = fds.poll_fds.get(data.poll_fds_index) {
                fds.fd_data
                    .get_mut(&swapped_pollfd.0.fd)
                    .unwrap()
                    .poll_fds_index = data.poll_fds_index;
            }

            Ok(())
        })
    }

    /// Perform a modification on `fds`, interrupting the current caller of `wait` if it's running.
    fn modify_fds<T>(&self, f: impl FnOnce(&mut Fds) -> io::Result<T>) -> io::Result<T> {
        self.waiting_operations.fetch_add(1, Ordering::SeqCst);

        // Wake up the current caller of `wait` if there is one.
        let sent_notification = self.notify_inner().is_ok();

        let mut fds = self.fds.lock().unwrap();

        // If there was no caller of `wait` our notification was not removed from the pipe.
        if sent_notification {
            let _ = self.pop_notification();
        }

        let res = f(&mut *fds);

        if self.waiting_operations.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.operations_complete.notify_one();
        }

        res
    }

    /// Wake the current thread that is calling `wait`.
    fn notify_inner(&self) -> io::Result<()> {
        syscall!(write(
            self.notify_write,
            &(1 as NotifyType) as *const _ as *const _,
            std::mem::size_of::<NotifyType>()
        ))?;
        Ok(())
    }

    /// Remove a notification created by `notify_inner`.
    fn pop_notification(&self) -> io::Result<()> {
        syscall!(read(
            self.notify_read,
            &mut [0; std::mem::size_of::<NotifyType>()] as *mut _ as *mut _,
            std::mem::size_of::<NotifyType>()
        ))?;
        Ok(())
    }

    #[cfg(debug_assertions)]
    pub fn register_waker(&self) -> bool {
        self.has_waker.swap(true, Ordering::AcqRel)
    }
}

cfg_io_source! {
    impl Selector {
        #[cfg(debug_assertions)]
        pub fn id(&self) -> usize {
            self.id
        }
    }
}

/// Get the input poll events for the given event.
fn interests_to_poll(interest: Interest) -> libc::c_short {
    let mut kind = 0;

    if interest.is_readable() {
        kind |= libc::POLLIN | libc::POLLPRI | libc::POLLHUP;
    }

    if interest.is_writable() {
        kind |= libc::POLLOUT | libc::POLLWRBAND;
    }

    kind
}

/// Helper function to call poll.
fn poll(fds: &mut [PollFd], deadline: Option<Instant>) -> io::Result<usize> {
    loop {
        // Convert the timeout to milliseconds.
        let timeout_ms = deadline
            .map(|deadline| {
                let timeout = deadline.saturating_duration_since(Instant::now());

                // Round up to a whole millisecond.
                let mut ms = timeout.as_millis().try_into().unwrap_or(u64::MAX);
                if Duration::from_millis(ms) < timeout {
                    ms = ms.saturating_add(1);
                }
                ms.try_into().unwrap_or(i32::MAX)
            })
            .unwrap_or(-1);

        log::trace!("Polling on {:?}", fds);
        let res = syscall!(poll(
            fds.as_mut_ptr() as *mut libc::pollfd,
            fds.len() as libc::nfds_t,
            timeout_ms,
        ));
        log::trace!("Polling finished: {:?} = {:?}", res, fds);

        match res {
            Ok(num_events) => break Ok(num_events as usize),
            // poll returns EAGAIN if we can retry it.
            Err(e) if e.raw_os_error() == Some(libc::EAGAIN) => continue,
            Err(e) => return Err(e),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    token: Token,
    events: libc::c_short,
}

pub type Events = Vec<Event>;

pub mod event {
    use crate::sys::Event;
    use crate::Token;
    use std::fmt;

    pub fn token(event: &Event) -> Token {
        event.token
    }

    pub fn is_readable(event: &Event) -> bool {
        (event.events & libc::POLLIN) != 0 || (event.events & libc::POLLPRI) != 0
    }

    pub fn is_writable(event: &Event) -> bool {
        (event.events & libc::POLLOUT) != 0
    }

    pub fn is_error(event: &Event) -> bool {
        (event.events & libc::POLLERR) != 0
    }

    pub fn is_read_closed(event: &Event) -> bool {
        // Both halves of the socket have closed
        (event.events & libc::POLLHUP) != 0
            // Socket has received FIN or called shutdown(SHUT_RD)
            || ((event.events & libc::POLLIN) != 0 && (event.events & libc::POLLRDHUP) != 0)
    }

    pub fn is_write_closed(event: &Event) -> bool {
        // Both halves of the socket have closed
        (event.events & libc::POLLHUP) != 0
            // Unix pipe write end has closed
            || ((event.events & libc::POLLOUT) != 0 && (event.events & libc::POLLERR) != 0)
            // The other side (read end) of a Unix pipe has closed.
            || (event.events == libc::POLLERR)
    }

    pub fn is_priority(event: &Event) -> bool {
        (event.events & libc::POLLPRI) != 0
    }

    pub fn is_aio(_: &Event) -> bool {
        // Not supported in the kernel, only in libc.
        false
    }

    pub fn is_lio(_: &Event) -> bool {
        // Not supported.
        false
    }

    pub fn debug_details(f: &mut fmt::Formatter<'_>, event: &Event) -> fmt::Result {
        #[allow(clippy::trivially_copy_pass_by_ref)]
        fn check_events(got: &libc::c_short, want: &libc::c_short) -> bool {
            (*got & want) != 0
        }
        debug_detail!(
            EventsDetails(libc::c_short),
            check_events,
            libc::POLLIN,
            libc::POLLPRI,
            libc::POLLOUT,
            libc::POLLRDNORM,
            libc::POLLRDBAND,
            libc::POLLWRNORM,
            libc::POLLWRBAND,
            libc::POLLERR,
            libc::POLLHUP,
            libc::POLLRDHUP,
        );

        // Can't reference fields in packed structures.
        let e_u64 = event.token.0;
        f.debug_struct("epoll_event")
            .field("events", &EventsDetails(event.events))
            .field("u64", &e_u64)
            .finish()
    }
}
