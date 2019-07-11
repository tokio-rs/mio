use std::os::unix::io::{AsRawFd, RawFd};
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{cmp, i32, io, ptr};

use libc::{EPOLLET, EPOLLIN, EPOLLOUT};
use log::error;

use crate::sys::Events;
use crate::{Interests, Token};

/// Unique id for use as `SelectorId`.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug)]
pub struct Selector {
    #[cfg(debug_assertions)]
    id: usize,
    ep: RawFd,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        // According to libuv `EPOLL_CLOEXEC` is not defined on Android API <
        // 21. But `EPOLL_CLOEXEC` is an alias for `O_CLOEXEC` on all platforms,
        // so we use that instead.
        syscall!(epoll_create1(libc::O_CLOEXEC)).map(|ep| Selector {
            #[cfg(debug_assertions)]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            ep,
        })
    }

    #[cfg(debug_assertions)]
    pub fn id(&self) -> usize {
        self.id
    }

    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        let timeout = timeout
            .map(|to| cmp::min(to.as_millis(), libc::c_int::max_value() as u128) as libc::c_int)
            .unwrap_or(-1);

        events.clear();
        syscall!(epoll_wait(
            self.ep,
            events.as_mut_ptr(),
            events.capacity() as i32,
            timeout,
        ))
        .map(|n_events| {
            // This is safe because `epoll_wait` ensures that `n_events` are
            // assigned.
            unsafe { events.set_len(n_events as usize) };
        })
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        let mut event = libc::epoll_event {
            events: interests_to_epoll(interests),
            u64: usize::from(token) as u64,
        };

        syscall!(epoll_ctl(self.ep, libc::EPOLL_CTL_ADD, fd, &mut event)).map(|_| ())
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        let mut event = libc::epoll_event {
            events: interests_to_epoll(interests),
            u64: usize::from(token) as u64,
        };

        syscall!(epoll_ctl(self.ep, libc::EPOLL_CTL_MOD, fd, &mut event)).map(|_| ())
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        syscall!(epoll_ctl(self.ep, libc::EPOLL_CTL_DEL, fd, ptr::null_mut())).map(|_| ())
    }
}

fn interests_to_epoll(interests: Interests) -> u32 {
    let mut kind = EPOLLET;

    if interests.is_readable() {
        kind |= EPOLLIN;
    }

    if interests.is_writable() {
        kind |= EPOLLOUT;
    }

    kind as u32
}

impl AsRawFd for Selector {
    fn as_raw_fd(&self) -> RawFd {
        self.ep
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        if let Err(err) = syscall!(close(self.ep)) {
            error!("error closing epoll: {}", err);
        }
    }
}

pub type Event = libc::epoll_event;

pub mod event {
    use crate::sys::Event;
    use crate::Token;

    pub fn token(event: &Event) -> Token {
        Token(event.u64 as usize)
    }

    pub fn is_readable(event: &Event) -> bool {
        (event.events as libc::c_int & libc::EPOLLIN) != 0
            || (event.events as libc::c_int & libc::EPOLLPRI) != 0
    }

    pub fn is_writable(event: &Event) -> bool {
        (event.events as libc::c_int & libc::EPOLLOUT) != 0
    }

    pub fn is_error(event: &Event) -> bool {
        (event.events as libc::c_int & libc::EPOLLERR) != 0
    }

    pub fn is_hup(event: &Event) -> bool {
        (event.events as libc::c_int & libc::EPOLLHUP) != 0
            || (event.events as libc::c_int & libc::EPOLLRDHUP) != 0
    }

    pub fn is_priority(event: &Event) -> bool {
        (event.events as libc::c_int & libc::EPOLLPRI) != 0
    }

    pub fn is_aio(_: &Event) -> bool {
        // Not supported in the kernel, only in libc.
        false
    }

    pub fn is_lio(_: &Event) -> bool {
        // Not supported.
        false
    }
}

#[test]
fn assert_close_on_exec_flag() {
    // This assertion need to be true for Selector::new.
    assert_eq!(libc::O_CLOEXEC, libc::EPOLL_CLOEXEC);
}
