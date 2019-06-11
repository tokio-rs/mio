use crate::event_imp::{self as event, Event};
use crate::sys::unix::cvt;
use crate::sys::unix::io::set_cloexec;
use crate::{io, Interests, Ready, Token};
use libc::{self, time_t};
use log::trace;
use std::collections::HashMap;
#[cfg(not(target_os = "netbsd"))]
use std::os::raw::{c_int, c_short};
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{cmp, fmt, ptr};

/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associated with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[cfg(not(target_os = "netbsd"))]
type Filter = c_short;
#[cfg(not(target_os = "netbsd"))]
type UData = *mut ::libc::c_void;
#[cfg(not(target_os = "netbsd"))]
type Count = c_int;

#[cfg(target_os = "netbsd")]
type Filter = u32;
#[cfg(target_os = "netbsd")]
type UData = ::libc::intptr_t;
#[cfg(target_os = "netbsd")]
type Count = usize;

macro_rules! kevent {
    ($id: expr, $filter: expr, $flags: expr, $data: expr) => {
        libc::kevent {
            ident: $id as ::libc::uintptr_t,
            filter: $filter as Filter,
            flags: $flags,
            fflags: 0,
            data: 0,
            udata: $data as UData,
        }
    };
}

pub struct Selector {
    id: usize,
    kq: RawFd,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        // offset by 1 to avoid choosing 0 as the id of a selector
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;
        let kq = unsafe { cvt(libc::kqueue())? };
        drop(set_cloexec(kq));

        Ok(Selector { id, kq })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn select(
        &self,
        evts: &mut Events,
        awakener: Token,
        timeout: Option<Duration>,
    ) -> io::Result<bool> {
        let timeout = timeout.map(|to| libc::timespec {
            tv_sec: cmp::min(to.as_secs(), time_t::max_value() as u64) as time_t,
            // `Duration::subsec_nanos` is guaranteed to be less than one
            // billion (the number of nanoseconds in a second), making the
            // cast to i32 safe. The cast itself is needed for platforms
            // where C's long is only 32 bits.
            tv_nsec: libc::c_long::from(to.subsec_nanos() as i32),
        });
        let timeout = timeout
            .as_ref()
            .map(|s| s as *const _)
            .unwrap_or(ptr::null_mut());

        evts.clear();
        unsafe {
            let cnt = cvt(libc::kevent(
                self.kq,
                ptr::null(),
                0,
                evts.sys_events.0.as_mut_ptr(),
                evts.sys_events.0.capacity() as Count,
                timeout,
            ))?;
            evts.sys_events.0.set_len(cnt as usize);
            Ok(evts.coalesce(awakener))
        }
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        trace!("registering; token={:?}; interests={:?}", token, interests);

        let flags = libc::EV_CLEAR | libc::EV_RECEIPT;

        unsafe {
            let r = if interests.is_readable() {
                libc::EV_ADD
            } else {
                libc::EV_DELETE
            };
            let w = if interests.is_writable() {
                libc::EV_ADD
            } else {
                libc::EV_DELETE
            };
            let mut changes = [
                kevent!(fd, libc::EVFILT_READ, flags | r, usize::from(token)),
                kevent!(fd, libc::EVFILT_WRITE, flags | w, usize::from(token)),
            ];

            cvt(libc::kevent(
                self.kq,
                changes.as_ptr(),
                changes.len() as Count,
                changes.as_mut_ptr(),
                changes.len() as Count,
                ::std::ptr::null(),
            ))?;

            for change in changes.iter() {
                debug_assert_eq!(change.flags & libc::EV_ERROR, libc::EV_ERROR);

                // Test to see if an error happened
                if change.data == 0 {
                    continue;
                }

                // Older versions of OSX (10.11 and 10.10 have been witnessed)
                // can return EPIPE when registering a pipe file descriptor
                // where the other end has already disappeared. For example code
                // that creates a pipe, closes a file descriptor, and then
                // registers the other end will see an EPIPE returned from
                // `register`.
                //
                // It also turns out that kevent will still report events on the
                // file descriptor, telling us that it's readable/hup at least
                // after we've done this registration. As a result we just
                // ignore `EPIPE` here instead of propagating it.
                //
                // More info can be found at tokio-rs/mio#582
                if change.data as i32 == libc::EPIPE
                    && change.filter == libc::EVFILT_WRITE as Filter
                {
                    continue;
                }

                // ignore ENOENT error for EV_DELETE
                let orig_flags = if change.filter == libc::EVFILT_READ as Filter {
                    r
                } else {
                    w
                };
                if change.data as i32 == libc::ENOENT && orig_flags & libc::EV_DELETE != 0 {
                    continue;
                }

                return Err(::std::io::Error::from_raw_os_error(change.data as i32));
            }
            Ok(())
        }
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        // Just need to call register here since EV_ADD is a mod if already
        // registered
        self.register(fd, token, interests)
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        unsafe {
            // EV_RECEIPT is a nice way to apply changes and get back per-event results while not
            // draining the actual changes.
            let filter = libc::EV_DELETE | libc::EV_RECEIPT;
            #[cfg(not(target_os = "netbsd"))]
            let mut changes = [
                kevent!(fd, libc::EVFILT_READ, filter, ptr::null_mut()),
                kevent!(fd, libc::EVFILT_WRITE, filter, ptr::null_mut()),
            ];

            #[cfg(target_os = "netbsd")]
            let mut changes = [
                kevent!(fd, libc::EVFILT_READ, filter, 0),
                kevent!(fd, libc::EVFILT_WRITE, filter, 0),
            ];

            cvt(libc::kevent(
                self.kq,
                changes.as_ptr(),
                changes.len() as Count,
                changes.as_mut_ptr(),
                changes.len() as Count,
                ::std::ptr::null(),
            ))
            .map(|_| ())?;

            if changes[0].data as i32 == libc::ENOENT && changes[1].data as i32 == libc::ENOENT {
                return Err(::std::io::Error::from_raw_os_error(changes[0].data as i32));
            }
            for change in changes.iter() {
                debug_assert_eq!(libc::EV_ERROR & change.flags, libc::EV_ERROR);
                if change.data != 0 && change.data as i32 != libc::ENOENT {
                    return Err(::std::io::Error::from_raw_os_error(changes[0].data as i32));
                }
            }
            Ok(())
        }
    }

    // Used by `Awakener`.
    #[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    pub fn setup_awakener(&self, token: Token) -> io::Result<()> {
        // First attempt to accept user space notifications.
        let mut kevent = kevent!(
            0,
            libc::EVFILT_USER,
            libc::EV_ADD | libc::EV_CLEAR | libc::EV_RECEIPT,
            token.0
        );

        cvt(unsafe {
            libc::kevent(
                self.kq,
                &kevent as *const libc::kevent,
                1,
                &mut kevent as *mut libc::kevent,
                1,
                ptr::null(),
            )
        })
        .map(|_| ())?;

        if (kevent.flags & libc::EV_ERROR) != 0 && kevent.data != 0 {
            Err(io::Error::from_raw_os_error(kevent.data as i32))
        } else {
            Ok(())
        }
    }

    // Used by `Awakener`.
    #[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    pub fn try_clone_awakener(&self) -> io::Result<Selector> {
        let new_kq = unsafe { libc::dup(self.kq) };
        if new_kq == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Selector {
                id: self.id,
                kq: new_kq,
            })
        }
    }

    // Used by `Awakener`.
    #[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
    pub fn wake(&self, token: Token) -> io::Result<()> {
        let mut kevent = kevent!(
            0,
            libc::EVFILT_USER,
            libc::EV_ADD | libc::EV_RECEIPT,
            token.0
        );
        kevent.fflags = libc::NOTE_TRIGGER;

        cvt(unsafe {
            libc::kevent(
                self.kq,
                &kevent as *const libc::kevent,
                1,
                &mut kevent as *mut libc::kevent,
                1,
                ptr::null(),
            )
        })
        .map(|_| ())?;

        if (kevent.flags & libc::EV_ERROR) != 0 && kevent.data != 0 {
            Err(io::Error::from_raw_os_error(kevent.data as i32))
        } else {
            Ok(())
        }
    }
}

impl fmt::Debug for Selector {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Selector")
            .field("id", &self.id)
            .field("kq", &self.kq)
            .finish()
    }
}

impl AsRawFd for Selector {
    fn as_raw_fd(&self) -> RawFd {
        self.kq
    }
}

impl Drop for Selector {
    fn drop(&mut self) {
        unsafe {
            let _ = libc::close(self.kq);
        }
    }
}

pub struct Events {
    sys_events: KeventList,
    events: Vec<Event>,
    event_map: HashMap<Token, usize>,
}

struct KeventList(Vec<libc::kevent>);

unsafe impl Send for KeventList {}
unsafe impl Sync for KeventList {}

impl Events {
    pub fn with_capacity(cap: usize) -> Events {
        Events {
            sys_events: KeventList(Vec::with_capacity(cap)),
            events: Vec::with_capacity(cap),
            event_map: HashMap::with_capacity(cap),
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

    pub fn get(&self, idx: usize) -> Option<Event> {
        self.events.get(idx).cloned()
    }

    fn coalesce(&mut self, awakener: Token) -> bool {
        let mut ret = false;
        self.events.clear();
        self.event_map.clear();

        for e in self.sys_events.0.iter() {
            let token = Token(e.udata as usize);
            let len = self.events.len();

            if token == awakener {
                // TODO: Should this return an error if event is an error. It
                // is not critical as spurious wakeups are permitted.
                ret = true;
                continue;
            }

            let idx = *self.event_map.entry(token).or_insert(len);

            if idx == len {
                // New entry, insert the default
                self.events.push(Event::new(Ready::EMPTY, token));
            }

            if e.flags & libc::EV_ERROR != 0 {
                event::kind_mut(&mut self.events[idx]).insert(Ready::ERROR);
            }

            if e.filter == libc::EVFILT_READ as Filter {
                event::kind_mut(&mut self.events[idx]).insert(Ready::READABLE);
            } else if e.filter == libc::EVFILT_WRITE as Filter {
                event::kind_mut(&mut self.events[idx]).insert(Ready::WRITABLE);
            }
            #[cfg(any(
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "ios",
                target_os = "macos"
            ))]
            {
                if e.filter == libc::EVFILT_AIO {
                    event::kind_mut(&mut self.events[idx]).insert(Ready::AIO);
                }
            }
            #[cfg(any(target_os = "freebsd"))]
            {
                if e.filter == libc::EVFILT_LIO {
                    event::kind_mut(&mut self.events[idx]).insert(Ready::LIO);
                }
            }

            // Used by the `Awakener`. On platforms that use `eventfd` or a unix
            // pipe it will emit a readable event so we'll fake that here as well.
            #[cfg(any(target_os = "freebsd", target_os = "ios", target_os = "macos"))]
            {
                if e.filter == libc::EVFILT_USER {
                    event::kind_mut(&mut self.events[idx]).insert(Ready::READABLE);
                }
            }
        }

        ret
    }

    pub fn push_event(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn clear(&mut self) {
        self.sys_events.0.truncate(0);
        self.events.truncate(0);
        self.event_map.clear();
    }
}

impl fmt::Debug for Events {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Events")
            .field("len", &self.sys_events.0.len())
            .finish()
    }
}

#[test]
fn does_not_register_rw() {
    use crate::unix::EventedFd;
    use crate::{Poll, Token};

    let kq = unsafe { libc::kqueue() };
    let kqf = EventedFd(&kq);
    let poll = Poll::new().unwrap();

    // registering kqueue fd will fail if write is requested (On anything but some versions of OS
    // X)
    poll.registry()
        .register(&kqf, Token(1234), Interests::READABLE)
        .unwrap();
}

#[cfg(any(
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "ios",
    target_os = "macos"
))]
#[test]
fn test_coalesce_aio() {
    let mut events = Events::with_capacity(1);
    events
        .sys_events
        .0
        .push(kevent!(0x1234, libc::EVFILT_AIO, 0, 42));
    events.coalesce(Token(0));
    assert!(events.events[0].readiness() == Ready::AIO);
    assert!(events.events[0].token() == Token(42));
}
