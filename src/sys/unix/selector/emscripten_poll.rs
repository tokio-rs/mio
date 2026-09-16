// A minimal, single-threaded, always-non-blocking `poll(2)`-based selector for Emscripten
// without atomics (no threads at all).
//
// The upstream `poll.rs` backend coordinates a real blocking `poll(2)` call against concurrent
// register/deregister from other threads via a `Mutex`+`Condvar`; without threads that condvar
// wait never wakes and traps. There's also no other thread to run the JS event loop that
// delivers new socket data while this one blocks in a syscall, so blocking here is never
// correct anyway. `select()` therefore always polls with a zero timeout and returns immediately,
// possibly with no events; waiting for real work is the caller's job (e.g. tokio's io driver,
// pumped from an external, JS-driven loop).

use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fmt, io};

use crate::sys::unix::waker::Waker as WakerInternal;
use crate::{Interest, Token};

#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
pub struct Selector {
    state: Arc<SelectorState>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        Ok(Selector {
            state: Arc::new(SelectorState::new()?),
        })
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        Ok(Selector {
            state: self.state.clone(),
        })
    }

    /// Non-blocking: always polls with a zero timeout and returns immediately, regardless of
    /// what `timeout` the caller asked for (see module comment). `events` may come back empty.
    pub fn select(&self, events: &mut Events, _timeout: Option<Duration>) -> io::Result<()> {
        self.state.select(events)
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        self.state.register(fd, token, interests)
    }

    pub(crate) fn register_internal(
        &self,
        fd: RawFd,
        token: Token,
        interests: Interest,
    ) -> io::Result<Arc<RegistrationRecord>> {
        self.state.register_internal(fd, token, interests)
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        self.state.reregister(fd, token, interests)
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        self.state.deregister(fd)
    }

    pub fn wake(&self, token: Token) -> io::Result<()> {
        self.state.wake(token)
    }

    #[cfg(debug_assertions)]
    pub fn id(&self) -> usize {
        self.state.id
    }
}

#[derive(Debug)]
struct SelectorState {
    fds: Mutex<Fds>,
    /// Self-pipe waker so `Waker::wake()` has an fd this selector reports readiness for like any
    /// other; we never block waiting on it ourselves.
    notify_waker: WakerInternal,
    pending_wake_token: Mutex<Option<Token>>,
    #[cfg(debug_assertions)]
    id: usize,
}

#[derive(Debug)]
struct Fds {
    poll_fds: Vec<PollFd>,
    fd_data: HashMap<RawFd, FdData>,
}

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

#[derive(Debug, Clone)]
struct FdData {
    poll_fds_index: usize,
    token: Token,
    shared_record: Arc<RegistrationRecord>,
}

impl SelectorState {
    fn new() -> io::Result<SelectorState> {
        let notify_waker = WakerInternal::new_unregistered()?;

        Ok(Self {
            fds: Mutex::new(Fds {
                poll_fds: if let Some(fd) = notify_waker.fd() {
                    vec![PollFd(libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    })]
                } else {
                    Vec::new()
                },
                fd_data: HashMap::new(),
            }),
            notify_waker,
            pending_wake_token: Mutex::new(None),
            #[cfg(debug_assertions)]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        })
    }

    fn select(&self, events: &mut Events) -> io::Result<()> {
        events.clear();

        let mut fds = self.fds.lock().unwrap();
        if fds.poll_fds.is_empty() {
            return Ok(());
        }

        let num_events = loop {
            match syscall_poll(&mut fds.poll_fds) {
                Ok(n) => break n,
                Err(e) if e.raw_os_error() == Some(libc::EAGAIN) => continue,
                Err(e) => return Err(e),
            }
        };
        if num_events == 0 {
            return Ok(());
        }

        let waker_events;
        let notified;
        let num_fd_events;
        if let Some(_waker_fd) = self.notify_waker.fd() {
            waker_events = fds.poll_fds[0].0.revents;
            notified = waker_events != 0;
            num_fd_events = if notified { num_events - 1 } else { num_events };
        } else {
            waker_events = 0;
            notified = self.notify_waker.woken();
            num_fd_events = num_events;
        }

        let pending_wake_token = self.pending_wake_token.lock().unwrap().take();
        let mut extra = 0;
        if notified {
            self.notify_waker.ack_and_reset();
            if pending_wake_token.is_some() {
                extra = 1;
            }
        }
        let num_fd_events = num_fd_events + extra;

        if num_fd_events == 0 {
            return Ok(());
        }

        events.reserve(num_fd_events);

        if let Some(pending_wake_token) = pending_wake_token {
            events.push(Event {
                token: pending_wake_token,
                events: waker_events,
            });
        }

        let mut closed_raw_fds = Vec::new();
        {
            let fds = &mut *fds;
            for fd_data in fds.fd_data.values_mut() {
                let PollFd(poll_fd) = &mut fds.poll_fds[fd_data.poll_fds_index];
                if poll_fd.revents != 0 {
                    events.push(Event {
                        token: fd_data.token,
                        events: poll_fd.revents,
                    });
                    if poll_fd.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                        closed_raw_fds.push(poll_fd.fd);
                    }
                    poll_fd.events &= !poll_fd.revents;
                    if events.len() == num_fd_events {
                        break;
                    }
                }
            }
        }
        drop(fds);

        // Sockets that hung up / errored won't produce further events: internally deregister
        // them now, mirroring upstream's poll.rs (IoSourceState treats a subsequent external
        // deregister as a no-op via `shared_record`).
        if !closed_raw_fds.is_empty() {
            let _ = self.deregister_all(&closed_raw_fds);
        }

        Ok(())
    }

    fn register(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        self.register_internal(fd, token, interests).map(|_| ())
    }

    fn register_internal(
        &self,
        fd: RawFd,
        token: Token,
        interests: Interest,
    ) -> io::Result<Arc<RegistrationRecord>> {
        #[cfg(debug_assertions)]
        if Some(fd) == self.notify_waker.fd() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }

        let mut fds = self.fds.lock().unwrap();
        if fds.fd_data.contains_key(&fd) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "I/O source already registered this `Registry` \
                (an old file descriptor might have been closed without deregistration)",
            ));
        }

        let poll_fds_index = fds.poll_fds.len();
        let record = Arc::new(RegistrationRecord::new());
        fds.fd_data.insert(
            fd,
            FdData {
                poll_fds_index,
                token,
                shared_record: record.clone(),
            },
        );
        let events = interests_to_poll(interests);
        fds.poll_fds.push(PollFd(libc::pollfd {
            fd,
            events,
            revents: 0,
        }));

        Ok(record)
    }

    fn reregister(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        let mut fds = self.fds.lock().unwrap();
        let data = fds.fd_data.get_mut(&fd).ok_or(io::ErrorKind::NotFound)?;
        data.token = token;
        let poll_fds_index = data.poll_fds_index;
        fds.poll_fds[poll_fds_index].0.events = interests_to_poll(interests);
        Ok(())
    }

    fn deregister(&self, fd: RawFd) -> io::Result<()> {
        self.deregister_all(&[fd])
            .map_err(|_| io::ErrorKind::NotFound)?;
        Ok(())
    }

    fn deregister_all(&self, targets: &[RawFd]) -> Result<(), ()> {
        if targets.is_empty() {
            return Ok(());
        }
        let mut fds = self.fds.lock().unwrap();
        let mut all_successful = true;
        for target in targets {
            match fds.fd_data.remove(target).ok_or(()) {
                Ok(data) => {
                    data.shared_record.mark_unregistered();
                    fds.poll_fds.swap_remove(data.poll_fds_index);
                    let swapped_fd = fds.poll_fds.get(data.poll_fds_index).map(|p| p.0.fd);
                    if let Some(swapped_fd) = swapped_fd {
                        fds.fd_data.get_mut(&swapped_fd).unwrap().poll_fds_index =
                            data.poll_fds_index;
                    }
                }
                Err(_) => all_successful = false,
            }
        }
        if all_successful {
            Ok(())
        } else {
            Err(())
        }
    }

    fn wake(&self, token: Token) -> io::Result<()> {
        self.pending_wake_token.lock().unwrap().replace(token);
        self.notify_waker.wake()
    }
}

type PollFlagInt = libc::c_short;

#[cfg(target_os = "linux")]
const POLLRDHUP: PollFlagInt = libc::POLLRDHUP;
#[cfg(not(target_os = "linux"))]
const POLLRDHUP: PollFlagInt = 0;

const POLLPRI: PollFlagInt = libc::POLLPRI;

const READ_EVENTS: PollFlagInt = libc::POLLIN | POLLRDHUP;
const WRITE_EVENTS: PollFlagInt = libc::POLLOUT;
const PRIORITY_EVENTS: PollFlagInt = POLLPRI;

fn interests_to_poll(interest: Interest) -> PollFlagInt {
    let mut kind = 0;
    if interest.is_readable() {
        kind |= READ_EVENTS;
    }
    if interest.is_writable() {
        kind |= WRITE_EVENTS;
    }
    if interest.is_priority() {
        kind |= PRIORITY_EVENTS;
    }
    kind
}

/// Raw, always-non-blocking `poll(2)` call (timeout always 0).
fn syscall_poll(fds: &mut [PollFd]) -> io::Result<usize> {
    let res = unsafe {
        libc::poll(
            fds.as_mut_ptr() as *mut libc::pollfd,
            fds.len() as libc::nfds_t,
            0,
        )
    };
    if res < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(res as usize)
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    token: Token,
    events: PollFlagInt,
}

pub type Events = Vec<Event>;

pub mod event {
    use std::fmt;

    use super::{POLLPRI, POLLRDHUP};
    use crate::sys::Event;
    use crate::Token;

    pub fn token(event: &Event) -> Token {
        event.token
    }

    pub fn is_readable(event: &Event) -> bool {
        (event.events & libc::POLLIN) != 0 || (event.events & POLLPRI) != 0
    }

    pub fn is_writable(event: &Event) -> bool {
        (event.events & libc::POLLOUT) != 0
    }

    pub fn is_error(event: &Event) -> bool {
        (event.events & libc::POLLERR) != 0
    }

    pub fn is_read_closed(event: &Event) -> bool {
        (event.events & libc::POLLHUP) != 0 || (event.events & POLLRDHUP) != 0
    }

    pub fn is_write_closed(event: &Event) -> bool {
        (event.events & libc::POLLHUP) != 0
            || ((event.events & libc::POLLOUT) != 0 && (event.events & libc::POLLERR) != 0)
            || (event.events == libc::POLLERR)
    }

    pub fn is_priority(event: &Event) -> bool {
        (event.events & POLLPRI) != 0
    }

    pub fn is_aio(_: &Event) -> bool {
        false
    }

    pub fn is_lio(_: &Event) -> bool {
        false
    }

    pub fn debug_details(f: &mut fmt::Formatter<'_>, event: &Event) -> fmt::Result {
        f.debug_struct("poll_event")
            .field("token", &event.token)
            .field("events", &event.events)
            .finish()
    }
}

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

mod registered_io_source;
pub(crate) use registered_io_source::IoSourceState;
pub(crate) use registered_io_source::RegistrationRecord;
