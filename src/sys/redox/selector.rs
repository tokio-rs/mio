use {io, Ready, PollOpt, Token};
use event::Event;
use syscall::{self, O_RDWR, O_CLOEXEC, EVENT_READ, close, fevent, read, open};
use std::mem;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use std::time::Duration;

/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associatd with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
static NEXT_ID: AtomicUsize = ATOMIC_USIZE_INIT;

#[derive(Debug)]
pub struct Selector {
    id: usize,
    efd: RawFd
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let efd = open("event:", O_RDWR | O_CLOEXEC).map_err(super::from_syscall_error)?;

        // offset by 1 to avoid choosing 0 as the id of a selector
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

        Ok(Selector {
            id: id,
            efd: efd,
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Wait for events from the OS
    pub fn select(&self, evts: &mut Events, awakener: Token, _timeout: Option<Duration>) -> io::Result<bool> {
        use std::slice;

        let dst = unsafe {
            slice::from_raw_parts_mut(
                evts.events.as_mut_ptr() as *mut u8,
                evts.events.capacity() * mem::size_of::<syscall::Event>())
        };

        let cnt = try!(read(self.efd, dst).map_err(super::from_syscall_error))
                                          / mem::size_of::<syscall::Event>();

        unsafe { evts.events.set_len(cnt); }

        for i in 0..cnt {
            if evts.get(i).map(|e| e.token()) == Some(awakener) {
                evts.events.remove(i);
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&self, fd: RawFd, _token: Token, interests: Ready, opts: PollOpt) -> io::Result<()> {
        let flags = ioevent_to_fevent(interests, opts);
        fevent(fd, flags).map_err(super::from_syscall_error).map(|_| ())
    }

    /// Register event interests for the given IO handle with the OS
    pub fn reregister(&self, fd: RawFd, _token: Token, interests: Ready, opts: PollOpt) -> io::Result<()> {
        let flags = ioevent_to_fevent(interests, opts);
        fevent(fd, flags).map_err(super::from_syscall_error).map(|_| ())
    }

    /// Deregister event interests for the given IO handle with the OS
    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        fevent(fd, 0).map_err(super::from_syscall_error).map(|_| ())
    }
}

fn ioevent_to_fevent(interest: Ready, _opts: PollOpt) -> usize {
    let mut flags = 0;

    if interest.is_readable() {
        flags |= EVENT_READ;
    }

    flags
}

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = close(self.efd);
    }
}

pub struct Events {
    events: Vec<syscall::Event>,
}

impl Events {
    pub fn with_capacity(u: usize) -> Events {
        Events { events: Vec::with_capacity(u) }
    }
    pub fn len(&self) -> usize {
        self.events.len()
    }
    pub fn capacity(&self) -> usize {
        self.events.capacity()
    }
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
    pub fn get(&self, idx: usize) -> Option<Event> {
        self.events.get(idx).map(|event| {
            let flags = event.flags;
            let mut kind = Ready::empty();

            if flags & EVENT_READ == EVENT_READ {
                kind = kind | Ready::readable();
            }

            let token = self.events[idx].id;

            Event::new(kind, Token(token as usize))
        })
    }
    pub fn push_event(&mut self, event: Event) {
        self.events.push(syscall::Event {
            id: usize::from(event.token()),
            flags: ioevent_to_fevent(event.readiness(), PollOpt::empty()),
            data: 0
        });
    }
}
