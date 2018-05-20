use {io, Ready, PollOpt, Token};
use event::Event;
use syscall::{self, O_RDWR, O_CLOEXEC, EVENT_READ, EVENT_WRITE, close, read, open, write};
use std::collections::{BTreeMap, BTreeSet};
use std::mem;
use std::os::unix::io::RawFd;
use std::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use std::sync::Mutex;
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
    efd: RawFd,
    tokens: Mutex<BTreeMap<RawFd, BTreeSet<Token>>>
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let efd = open("event:", O_RDWR | O_CLOEXEC).map_err(super::from_syscall_error)?;

        // offset by 1 to avoid choosing 0 as the id of a selector
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;

        Ok(Selector {
            id: id,
            efd: efd,
            tokens: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Wait for events from the OS
    pub fn select(&self, evts: &mut Events, awakener: Token, _timeout: Option<Duration>) -> io::Result<bool> {
        use std::slice;

        let cnt;
        unsafe {
            let bytes = read(self.efd, slice::from_raw_parts_mut(
                evts.events.as_mut_ptr() as *mut u8,
                evts.events.capacity() * mem::size_of::<syscall::Event>()
            )).map_err(super::from_syscall_error)?;
            cnt = bytes / mem::size_of::<syscall::Event>();

            evts.events.set_len(cnt);
        }

        for i in 0..evts.len() {
            if evts.events.get(i).map(|e| e.data) == Some(awakener.into()) {
                evts.events.remove(i);
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn inner_register(&self, fd: RawFd, token: Token, flags: usize) -> io::Result<()> {
        write(self.efd, &syscall::Event {
            id: fd as usize,
            flags: flags,
            data: token.into()
        })
        .map(|_| ())
        .map_err(super::from_syscall_error)
    }

    /// Register event interests for the given IO handle with the OS
    pub fn register(&self, fd: RawFd, token: Token, interests: Ready, _opts: PollOpt) -> io::Result<()> {
        self.inner_register(fd, token, ioevent_to_fevent(interests))?;

        let mut tokens = self.tokens.lock().unwrap();
        tokens.entry(fd).or_insert_with(BTreeSet::new).insert(token);

        Ok(())
    }

    /// Register event interests for the given IO handle with the OS
    pub fn reregister(&self, fd: RawFd, token: Token, interests: Ready, opts: PollOpt) -> io::Result<()> {
        self.register(fd, token, interests, opts)
    }

    /// Deregister event interests for the given IO handle with the OS
    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        let mut tokens = self.tokens.lock().unwrap();

        if let Some(tokens) = tokens.remove(&fd) {
            for token in tokens {
                self.inner_register(fd, token, 0)?;
            }
        }

        Ok(())
    }
}

fn ioevent_to_fevent(interest: Ready) -> usize {
    let mut flags = 0;

    if interest.is_readable() {
        flags |= EVENT_READ;
    }
    if interest.is_writable() {
        flags |= EVENT_WRITE;
    }

    flags
}
fn fevent_to_ioevent(flags: usize) -> Ready {
    let mut kind = Ready::empty();

    if flags & EVENT_READ == EVENT_READ {
        kind = kind | Ready::readable();
    }
    if flags & EVENT_WRITE == EVENT_WRITE {
        kind = kind | Ready::writable();
    }

    kind
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
        let event = self.events.get(idx)?;

        Some(Event::new(
            fevent_to_ioevent(event.flags),
            Token::from(event.data)
        ))
    }
    pub fn push_event(&mut self, event: Event) {
        self.events.push(syscall::Event {
            id: 0,
            flags: ioevent_to_fevent(event.readiness()),
            data: event.token().into()
        })
    }
    pub fn clear(&mut self) {
        self.events.clear();
    }
}
