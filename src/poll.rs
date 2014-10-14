use std::fmt;
use error::MioResult;
use io::IoHandle;
use os;
use token::Token;
use handler::{ReadHint, DATAHINT, HUPHINT, ERRORHINT};

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

    pub fn register<H: IoHandle>(&mut self, io: &H, token: Token) -> MioResult<()> {
        debug!("registering IO with poller");

        // Register interests for this socket
        try!(self.selector.register(io.desc(), token.as_uint()));

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
    flags IoEventKind: uint {
        const IOREADABLE = 0x001,
        const IOWRITABLE = 0x002,
        const IOERROR    = 0x004,
        const IOHUPHINT  = 0x008,
        const IOHINTED   = 0x010
    }
)

impl fmt::Show for IoEventKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (IOREADABLE, "IoReadable"),
            (IOWRITABLE, "IoWritable"),
            (IOERROR, "IoError"),
            (IOHUPHINT, "IoHupHint"),
            (IOHINTED, "IoHinted")];

        for &(flag, msg) in flags.iter() {
            if self.contains(flag) {
                if one { try!(write!(fmt, " | ")) }
                try!(write!(fmt, "{}", msg));

                one = true
            }
        }

        Ok(())
    }
}

#[deriving(Show)]
pub struct IoEvent {
    kind: IoEventKind,
    token: Token
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
            token: Token(token)
        }
    }

    pub fn token(&self) -> Token {
        self.token
    }

    /// Return an optional hint for a readable IO handle. Currently,
    /// this method supports the HupHint, which indicates that the
    /// kernel reported that the remote side hung up. This allows a
    /// consumer to avoid reading in order to discover the hangup.
    pub fn read_hint(&self) -> ReadHint {
        let mut hint = ReadHint::empty();

        // The backend doesn't support hinting
        if !self.kind.contains(IOHINTED) {
            return hint;
        }

        if self.kind.contains(IOHUPHINT) {
            hint = hint | HUPHINT
        }

        if self.kind.contains(IOREADABLE) {
            hint = hint | DATAHINT
        }

        if self.kind.contains(IOERROR) {
            hint = hint | ERRORHINT
        }

        hint
    }

    /// This event indicated that the IO handle is now readable
    pub fn is_readable(&self) -> bool {
        self.kind.contains(IOREADABLE) || self.kind.contains(IOHUPHINT)
    }

    /// This event indicated that the IO handle is now writable
    pub fn is_writable(&self) -> bool {
        self.kind.contains(IOWRITABLE)
    }

    /// This event indicated that the IO handle had an error
    pub fn is_error(&self) -> bool {
        self.kind.contains(IOERROR)
    }
}
