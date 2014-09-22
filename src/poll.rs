use error::MioResult;
use nix::fcntl::Fd;
use os;

pub struct Poll {
    selector: os::Selector,
    events: os::Events
}

impl Poll {
    pub fn new() -> MioResult<Poll> {
        Ok(Poll {
            selector: try!(os::Selector::new()),
            events: os::Events::new()
        })
    }

    pub fn register(&mut self, fd: Fd, token: uint) -> MioResult<()> {
        debug!("registering IO with poller");
        self.selector.register(fd, token)
    }

    pub fn unregister(&mut self, fd: Fd) -> MioResult<()> {
        self.selector.unregister(fd)
    }

    pub fn poll(&mut self, timeout_ms: uint) -> MioResult<uint> {
        try!(self.selector.select(&mut self.events, timeout_ms));
        Ok(self.events.len())
    }

    pub fn event(&self, idx: uint) -> IoEvent {
        self.events.get(idx)
    }
}


bitflags!(
    #[deriving(Show)]
    flags IoEventKind: uint {
        static IoReadable = 0x001,
        static IoWritable = 0x002,
        static IoError    = 0x004
    }
)

#[deriving(Show)]
pub struct IoEvent {
    kind:  IoEventKind,
    token: uint,
}

/// IoEvent represents the raw event that the OS-specific selector
/// returned. An event can represent more than one kind (such as
/// readable or writable) at a time.
///
/// These IoEvent objects are created by the OS-specific concrete
/// Selector when they have events to report.
impl IoEvent {
    /// Create a new IoEvent.
    pub fn new(kind: IoEventKind, token: uint) -> IoEvent {
        IoEvent {
            kind: kind,
            token: token
        }
    }

    #[inline(always)]
    pub fn token(&self) -> uint {
        self.token
    }

    /// This event indicated that the IO handle is now readable
    #[inline(always)]
    pub fn is_readable(&self) -> bool {
        self.kind.contains(IoReadable)
    }

    #[inline(always)]
    /// This event indicated that the IO handle is now writable
    pub fn is_writable(&self) -> bool {
        self.kind.contains(IoWritable)
    }

    #[inline(always)]
    /// This event indicated that the IO handle had an error
    pub fn is_error(&self) -> bool {
        self.kind.contains(IoError)
    }
}
