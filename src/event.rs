use token::Token;
use std::{fmt, ops};

#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct PollOpt(usize);

impl PollOpt {
    #[inline]
    pub fn edge() -> PollOpt {
        PollOpt(0x020)
    }

    #[inline]
    pub fn empty() -> PollOpt {
        PollOpt(0)
    }

    #[inline]
    pub fn level() -> PollOpt {
        PollOpt(0x040)
    }

    #[inline]
    pub fn oneshot() -> PollOpt {
        PollOpt(0x080)
    }

    #[inline]
    pub fn all() -> PollOpt {
        PollOpt::edge() | PollOpt::level() | PollOpt::oneshot()
    }

    #[inline]
    pub fn is_edge(&self) -> bool {
        self.contains(PollOpt::edge())
    }

    #[inline]
    pub fn is_level(&self) -> bool {
        self.contains(PollOpt::level())
    }

    #[inline]
    pub fn is_oneshot(&self) -> bool {
        self.contains(PollOpt::oneshot())
    }

    #[inline]
    pub fn bits(&self) -> usize {
        self.0
    }

    #[inline]
    pub fn contains(&self, other: PollOpt) -> bool {
        (*self & other) == other
    }

    #[inline]
    pub fn insert(&mut self, other: PollOpt) {
        self.0 |= other.0;
    }

    #[inline]
    pub fn remove(&mut self, other: PollOpt) {
        self.0 &= !other.0;
    }
}

impl ops::BitOr for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn bitor(self, other: PollOpt) -> PollOpt {
        PollOpt(self.bits() | other.bits())
    }
}

impl ops::BitXor for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn bitxor(self, other: PollOpt) -> PollOpt {
        PollOpt(self.bits() ^ other.bits())
    }
}

impl ops::BitAnd for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn bitand(self, other: PollOpt) -> PollOpt {
        PollOpt(self.bits() & other.bits())
    }
}

impl ops::Sub for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn sub(self, other: PollOpt) -> PollOpt {
        PollOpt(self.bits() & !other.bits())
    }
}

impl ops::Not for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn not(self) -> PollOpt {
        PollOpt(!self.bits() & PollOpt::all().bits())
    }
}

impl fmt::Debug for PollOpt {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (PollOpt::edge(), "Edge-Triggered"),
            (PollOpt::level(), "Level-Triggered"),
            (PollOpt::oneshot(), "OneShot")];

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

#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct EventSet(usize);

impl EventSet {
    pub fn none() -> EventSet {
        EventSet(0)
    }

    #[inline]
    pub fn readable() -> EventSet {
        EventSet(0x001)
    }

    #[inline]
    pub fn writable() -> EventSet {
        EventSet(0x002)
    }

    #[inline]
    pub fn error() -> EventSet {
        EventSet(0x004)
    }

    #[inline]
    pub fn hup() -> EventSet {
        EventSet(0x008)
    }

    #[inline]
    pub fn all() -> EventSet {
        EventSet::readable() |
            EventSet::writable() |
            EventSet::hup() |
            EventSet::error()
    }

    #[inline]
    pub fn is_readable(&self) -> bool {
        self.contains(EventSet::readable())
    }

    #[inline]
    pub fn is_writable(&self) -> bool {
        self.contains(EventSet::writable())
    }

    #[inline]
    pub fn is_error(&self) -> bool {
        self.contains(EventSet::error())
    }

    #[inline]
    pub fn is_hup(&self) -> bool {
        self.contains(EventSet::hup())
    }

    #[inline]
    pub fn insert(&mut self, other: EventSet) {
        self.0 |= other.0;
    }

    #[inline]
    pub fn remove(&mut self, other: EventSet) {
        self.0 &= !other.0;
    }

    #[inline]
    pub fn bits(&self) -> usize {
        self.0
    }

    #[inline]
    pub fn contains(&self, other: EventSet) -> bool {
        (*self & other) == other
    }
}

impl ops::BitOr for EventSet {
    type Output = EventSet;

    #[inline]
    fn bitor(self, other: EventSet) -> EventSet {
        EventSet(self.bits() | other.bits())
    }
}

impl ops::BitXor for EventSet {
    type Output = EventSet;

    #[inline]
    fn bitxor(self, other: EventSet) -> EventSet {
        EventSet(self.bits() ^ other.bits())
    }
}

impl ops::BitAnd for EventSet {
    type Output = EventSet;

    #[inline]
    fn bitand(self, other: EventSet) -> EventSet {
        EventSet(self.bits() & other.bits())
    }
}

impl ops::Sub for EventSet {
    type Output = EventSet;

    #[inline]
    fn sub(self, other: EventSet) -> EventSet {
        EventSet(self.bits() & !other.bits())
    }
}

impl ops::Not for EventSet {
    type Output = EventSet;

    #[inline]
    fn not(self) -> EventSet {
        EventSet(!self.bits() & EventSet::all().bits())
    }
}

impl fmt::Debug for EventSet {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (EventSet::readable(), "Readable"),
            (EventSet::writable(), "Writable"),
            (EventSet::error(),    "Error"),
            (EventSet::hup(),      "Hup")];

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

// Keep this struct internal to mio
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct IoEvent {
    pub kind: EventSet,
    pub token: Token
}

/// IoEvent represents the raw event that the OS-specific selector
/// returned. An event can represent more than one kind (such as
/// readable or writable) at a time.
///
/// These IoEvent objects are created by the OS-specific concrete
/// Selector when they have events to report.
impl IoEvent {
    /// Create a new IoEvent.
    pub fn new(kind: EventSet, token: Token) -> IoEvent {
        IoEvent {
            kind: kind,
            token: token,
        }
    }
}
