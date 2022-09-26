use crate::{Interest, Token};
use log::error;
use std::os::unix::io::{AsRawFd, RawFd};
use std::time::Duration;
use std::io;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
pub struct Selector {
    #[cfg(debug_assertions)]
    id: usize,
    ps: RawFd,
    #[cfg(debug_assertions)]
    has_waker: AtomicBool,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        syscall!(pollset_create(1024)).map(|ps| Selector {
            #[cfg(debug_assertions)]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            ps,
            #[cfg(debug_assertions)]
            has_waker: AtomicBool::new(false),
        })
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        syscall!(fcntl(self.ps, libc::F_DUPFD_CLOEXEC, super::LOWEST_FD)).map(|ps| Selector {
            // It's the same selector, so we use the same id.
            #[cfg(debug_assertions)]
            id: self.id,
            ps,
            #[cfg(debug_assertions)]
            has_waker: AtomicBool::new(self.has_waker.load(Ordering::Acquire)),
        })
    }

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();
        syscall!(pollset_poll(
            self.ps,
            events.as_mut_ptr(),
            events.capacity() as i32,
            -1,
        ))
        .map(|n_events| {
            unsafe { events.set_len(n_events as usize) };
        })
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        let mut control: [libc::poll_ctl; 1] = [libc::poll_ctl {
            cmd: libc::PS_ADD as i16,
            events: interests_to_pollset(interests),
            fd: fd as i32,
        }; 1];
        syscall!(pollset_ctl(self.ps, control.as_mut_ptr(), 1)).map(|_| ())
    }

    // TODO: PS_MOD or PS_REPLACE?
    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        let mut control: [libc::poll_ctl; 1] = [libc::poll_ctl {
            cmd: libc::PS_MOD as i16,
            events: interests_to_pollset(interests),
            fd: fd as i32,
        }; 1];
        syscall!(pollset_ctl(self.ps, control.as_mut_ptr(), 1)).map(|_| ())
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        let mut control: [libc::poll_ctl; 1] = [libc::poll_ctl {
            cmd: libc::PS_DELETE as i16,
            events: 0,
            fd: fd as i32,
        }; 1];
        syscall!(pollset_ctl(self.ps, control.as_mut_ptr(), 1)).map(|_| ())
    }

    #[cfg(debug_assertions)]
    pub fn register_waker(&self) -> bool {
        self.has_waker.swap(true, Ordering::AcqRel)
    }
}

impl AsRawFd for Selector {
    fn as_raw_fd(&self) -> RawFd {
        self.ps
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        if let Err(err) = syscall!(pollset_destroy(self.ps)) {
            error!("error closing pollset: {}", err);
        }
    }
}

fn interests_to_pollset(interests: Interest) -> i16 {
    let mut kind: i16 = 0;
    if interests.is_readable() {
        kind |= libc::POLLIN;
    }
    if interests.is_writable() {
        kind |= libc::POLLOUT;
    }
    kind
}

pub type Event = libc::pollfd;
pub type Events = Vec<Event>;

pub mod event {
    use std::fmt;

    use crate::sys::Event;
    use crate::Token;

    pub fn token(event: &Event) -> Token {
        Token(event.fd as usize)
    }

    pub fn is_readable(event: &Event) -> bool {
        (event.revents & libc::POLLIN) != 0
    }

    pub fn is_writable(event: &Event) -> bool {
        (event.revents & libc::POLLOUT) != 0
    }

    pub fn is_error(event: &Event) -> bool {
        false
    }

    pub fn is_read_closed(event: &Event) -> bool {
        false
    }

    pub fn is_write_closed(event: &Event) -> bool {
        false
    }

    pub fn is_priority(event: &Event) -> bool {
        (event.revents & libc::POLLPRI) != 0
    }

    pub fn is_aio(_: &Event) -> bool {
        false
    }

    pub fn is_lio(_: &Event) -> bool {
        false
    }

    pub fn debug_details(f: &mut fmt::Formatter<'_>, event: &Event) -> fmt::Result {
        unimplemented!()
    }
}
