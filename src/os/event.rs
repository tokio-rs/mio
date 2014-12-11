use std::{fmt};
use os::token::Token;

#[deriving(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct PollOpt(uint);

pub const EDGE: PollOpt    = PollOpt(0x020);
pub const LEVEL: PollOpt   = PollOpt(0x040);
pub const ONESHOT: PollOpt = PollOpt(0x080);

impl PollOpt {
    #[inline]
    pub fn edge() -> PollOpt {
        EDGE | ONESHOT
    }

    #[inline]
    pub fn empty() -> PollOpt {
        PollOpt(0)
    }

    #[inline]
    pub fn all() -> PollOpt {
        EDGE | LEVEL | ONESHOT
    }

    #[inline]
    pub fn bits(&self) -> uint {
        let PollOpt(bits) = *self;
        bits
    }

    #[inline]
    pub fn contains(&self, other: PollOpt) -> bool {
        (*self & other) == other
    }
}

impl BitOr<PollOpt, PollOpt> for PollOpt {
    #[inline]
    fn bitor(&self, other: &PollOpt) -> PollOpt {
        PollOpt(self.bits() | other.bits())
    }
}

impl BitXor<PollOpt, PollOpt> for PollOpt {
    #[inline]
    fn bitxor(&self, other: &PollOpt) -> PollOpt {
        PollOpt(self.bits() ^ other.bits())
    }
}

impl BitAnd<PollOpt, PollOpt> for PollOpt {
    #[inline]
    fn bitand(&self, other: &PollOpt) -> PollOpt {
        PollOpt(self.bits() & other.bits())
    }
}

impl Sub<PollOpt, PollOpt> for PollOpt {
    #[inline]
    fn sub(&self, other: &PollOpt) -> PollOpt {
        PollOpt(self.bits() & !other.bits())
    }
}

impl Not<PollOpt> for PollOpt {
    #[inline]
    fn not(&self) -> PollOpt {
        PollOpt(!self.bits() & PollOpt::all().bits())
    }
}

impl fmt::Show for PollOpt {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (EDGE, "Edge-Triggered"),
            (LEVEL, "Level-Triggered"),
            (ONESHOT, "OneShot")];

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

bitflags!(
    #[deriving(Copy)]
    flags Interest: uint {
        const READABLE = 0x001,
        const WRITABLE = 0x002,
        const ERROR    = 0x004,
        const HUP      = 0x008,
        const HINTED   = 0x010,
        const ALL      = 0x001 | 0x002 | 0x008  //epoll checks for ERROR no matter what
    }
)


impl fmt::Show for Interest {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (READABLE, "Readable"),
            (WRITABLE, "Writable"),
            (ERROR,    "Error"),
            (HUP,      "HupHint"),
            (HINTED,   "Hinted")];

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

bitflags!(
    #[deriving(Copy)]
    flags ReadHint: uint {
        const DATAHINT    = 0x001,
        const HUPHINT     = 0x002,
        const ERRORHINT   = 0x004
    }
)

impl fmt::Show for ReadHint {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (DATAHINT, "DataHint"),
            (HUPHINT, "HupHint"),
            (ERRORHINT, "ErrorHint")];

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


#[deriving(Copy, Show)]
pub struct IoEvent {
    kind: Interest,
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
    pub fn new(kind: Interest, token: uint) -> IoEvent {
        IoEvent {
            kind: kind,
            token: Token(token)
        }
    }

    pub fn token(&self) -> Token {
        self.token
    }

    /// Return an optional hint for a readable  handle. Currently,
    /// this method supports the HupHint, which indicates that the
    /// kernel reported that the remote side hung up. This allows a
    /// consumer to avoid reading in order to discover the hangup.
    pub fn read_hint(&self) -> ReadHint {
        let mut hint = ReadHint::empty();

        // The backend doesn't support hinting
        if !self.kind.contains(HINTED) {
            return hint;
        }

        if self.kind.contains(HUP) {
            hint = hint | HUPHINT
        }

        if self.kind.contains(READABLE) {
            hint = hint | DATAHINT
        }

        if self.kind.contains(ERROR) {
            hint = hint | ERRORHINT
        }

        hint
    }

    /// This event indicated that the  handle is now readable
    pub fn is_readable(&self) -> bool {
        self.kind.contains(READABLE) || self.kind.contains(HUP)
    }

    /// This event indicated that the  handle is now writable
    pub fn is_writable(&self) -> bool {
        self.kind.contains(WRITABLE)
    }

    /// This event indicated that the  handle had an error
    pub fn is_error(&self) -> bool {
        self.kind.contains(ERROR)
    }
}
