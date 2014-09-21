use error::MioResult;
use io::IoHandle;
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

    pub fn register<H: IoHandle>(&mut self, io: &H, token: u64) -> MioResult<()> {
        debug!("registering IO with poller");

        // Register interests for this socket
        try!(self.selector.register(io.desc(), token));

        Ok(())
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
    kind: IoEventKind,
    token: u64
}

/// IoEvent represents the raw event that the OS-specific selector
/// returned. An event can represent more than one kind (such as
/// readable or writable) at a time.
///
/// These IoEvent objects are created by the OS-specific concrete
/// Selector when they have events to report.
impl IoEvent {
    /// Create a new IoEvent.
    pub fn new(kind: IoEventKind, token: u64) -> IoEvent {
        IoEvent {
            kind: kind,
            token: token
        }
    }

    pub fn token(&self) -> u64 {
        self.token
    }

    /// This event indicated that the IO handle is now readable
    pub fn is_readable(&self) -> bool {
        self.kind.contains(IoReadable)
    }

    /// This event indicated that the IO handle is now writable
    pub fn is_writable(&self) -> bool {
        self.kind.contains(IoWritable)
    }

    /// This event indicated that the IO handle had an error
    pub fn is_error(&self) -> bool {
        self.kind.contains(IoError)
    }
}
