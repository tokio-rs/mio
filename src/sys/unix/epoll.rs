use crate::sys::unix::cvt;
use crate::{Interests, Token};

use libc::{EPOLLET, EPOLLIN, EPOLLOUT};
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{cmp, i32, io};

/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associated with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct Selector {
    #[cfg(debug_assertions)]
    id: usize,
    epfd: RawFd,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        // According to libuv `EPOLL_CLOEXEC` is not defined on Android API <
        // 21. But `EPOLL_CLOEXEC` is an alias for `O_CLOEXEC` on all platforms,
        // so we use that instead.
        let epfd = cvt(unsafe { libc::epoll_create1(libc::O_CLOEXEC) })?;

        // offset by 1 to avoid choosing 0 as the id of a selector
        #[cfg(debug_assertions)]
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

        Ok(Selector {
            #[cfg(debug_assertions)]
            id,
            epfd,
        })
    }

    #[cfg(debug_assertions)]
    pub fn id(&self) -> usize {
        self.id
    }

    /// Wait for events from the OS
    pub fn select(&self, evts: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        let timeout_ms = timeout
            .map(|to| cmp::min(millis(to), i32::MAX as u64) as i32)
            .unwrap_or(-1);

        // Wait for epoll events for at most timeout_ms milliseconds
        evts.clear();
        unsafe {
            let cnt = cvt(libc::epoll_wait(
                self.epfd,
                evts.events.as_mut_ptr(),
                evts.events.capacity() as i32,
                timeout_ms,
            ))?;
            let cnt = cnt as usize;
            evts.events.set_len(cnt);
        }

        Ok(())
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        let mut info = libc::epoll_event {
            events: interests_to_epoll(interests),
            u64: usize::from(token) as u64,
        };

        unsafe {
            cvt(libc::epoll_ctl(
                self.epfd,
                libc::EPOLL_CTL_ADD,
                fd,
                &mut info,
            ))?;
            Ok(())
        }
    }

    /// Register event interests for the given IO handle with the OS
    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        let mut info = libc::epoll_event {
            events: interests_to_epoll(interests),
            u64: usize::from(token) as u64,
        };

        unsafe {
            cvt(libc::epoll_ctl(
                self.epfd,
                libc::EPOLL_CTL_MOD,
                fd,
                &mut info,
            ))?;
            Ok(())
        }
    }

    /// Deregister event interests for the given IO handle with the OS
    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        // The &info argument should be ignored by the system,
        // but linux < 2.6.9 required it to be not null.
        // For compatibility, we provide a dummy EpollEvent.
        let mut info = libc::epoll_event { events: 0, u64: 0 };

        unsafe {
            cvt(libc::epoll_ctl(
                self.epfd,
                libc::EPOLL_CTL_DEL,
                fd,
                &mut info,
            ))?;
            Ok(())
        }
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
        self.epfd
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::close(self.epfd);
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

pub struct Events {
    events: Vec<libc::epoll_event>,
}

impl Events {
    pub fn with_capacity(u: usize) -> Events {
        Events {
            events: Vec::with_capacity(u),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.events.capacity()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Option<&Event> {
        self.events.get(idx)
    }

    pub fn clear(&mut self) {
        unsafe {
            self.events.set_len(0);
        }
    }
}

const NANOS_PER_MILLI: u32 = 1_000_000;
const MILLIS_PER_SEC: u64 = 1_000;

/// Convert a `Duration` to milliseconds, rounding up and saturating at
/// `u64::MAX`.
///
/// The saturating is fine because `u64::MAX` milliseconds are still many
/// million years.
pub fn millis(duration: Duration) -> u64 {
    // Round up.
    let millis = (duration.subsec_nanos() + NANOS_PER_MILLI - 1) / NANOS_PER_MILLI;
    duration
        .as_secs()
        .saturating_mul(MILLIS_PER_SEC)
        .saturating_add(millis as u64)
}

#[test]
fn assert_close_on_exec_flag() {
    // This assertion need to be true for Selector::new.
    assert_eq!(libc::O_CLOEXEC, libc::EPOLL_CLOEXEC);
}
