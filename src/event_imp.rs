use token::Token;
use std::{fmt, ops};

/// Configures readiness polling behavior for a given `Evented` value.
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct PollOpt(usize);

impl PollOpt {
    #[inline]
    pub fn empty() -> PollOpt {
        PollOpt(0)
    }

    #[inline]
    pub fn edge() -> PollOpt {
        PollOpt(0b0001)
    }

    #[inline]
    pub fn level() -> PollOpt {
        PollOpt(0b0010)
    }

    #[inline]
    pub fn oneshot() -> PollOpt {
        PollOpt(0b0100)
    }

    #[inline]
    pub fn urgent() -> PollOpt {
        PollOpt(0b1000)
    }

    #[deprecated(since = "0.6.5", note = "removed")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
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
    pub fn is_urgent(&self) -> bool {
        self.contains(PollOpt::urgent())
    }

    #[deprecated(since = "0.6.5", note = "removed")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
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
        PollOpt(self.0 | other.0)
    }
}

impl ops::BitXor for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn bitxor(self, other: PollOpt) -> PollOpt {
        PollOpt(self.0 ^ other.0)
    }
}

impl ops::BitAnd for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn bitand(self, other: PollOpt) -> PollOpt {
        PollOpt(self.0 & other.0)
    }
}

impl ops::Sub for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn sub(self, other: PollOpt) -> PollOpt {
        PollOpt(self.0 & !other.0)
    }
}

impl ops::Not for PollOpt {
    type Output = PollOpt;

    #[inline]
    fn not(self) -> PollOpt {
        PollOpt(!self.0)
    }
}

impl fmt::Debug for PollOpt {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (PollOpt::edge(), "Edge-Triggered"),
            (PollOpt::level(), "Level-Triggered"),
            (PollOpt::oneshot(), "OneShot")];

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one { try!(write!(fmt, " | ")) }
                try!(write!(fmt, "{}", msg));

                one = true
            }
        }

        Ok(())
    }
}

/// A set of readiness events returned by `Poll`.
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Ready(usize);

const READABLE: usize = 0b0001;
const WRITABLE: usize = 0b0010;
const ERROR: usize    = 0b0100;
const HUP: usize      = 0b1000;
const READY_ALL: usize = READABLE | WRITABLE | ERROR | HUP;

pub trait ReadyUnix {
    fn error() -> Self;

    fn hup() -> Self;

    fn is_error(&self) -> bool;

    #[inline]
    fn is_hup(&self) -> bool;
}

impl Ready {
    pub fn none() -> Ready {
        Ready(0)
    }

    #[inline]
    pub fn readable() -> Ready {
        Ready(READABLE)
    }

    #[inline]
    pub fn writable() -> Ready {
        Ready(WRITABLE)
    }

    #[deprecated(since = "0.6.5", note = "use unix::ReadyExt instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn error() -> Ready {
        Ready(ERROR)
    }

    #[deprecated(since = "0.6.5", note = "use unix::ReadyExt instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn hup() -> Ready {
        Ready(HUP)
    }

    #[inline]
    pub fn all() -> Ready {
        Ready::readable() |
            Ready::writable()
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        *self == Ready::none()
    }

    #[inline]
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    #[inline]
    pub fn is_readable(&self) -> bool {
        self.contains(Ready::readable())
    }

    #[inline]
    pub fn is_writable(&self) -> bool {
        self.contains(Ready::writable())
    }

    #[deprecated(since = "0.6.5", note = "use unix::ReadyExt instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn is_error(&self) -> bool {
        self.contains(Ready(ERROR))
    }

    #[deprecated(since = "0.6.5", note = "use unix::ReadyExt instead")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn is_hup(&self) -> bool {
        self.contains(Ready(HUP))
    }

    #[inline]
    pub fn insert(&mut self, other: Ready) {
        self.0 |= other.0;
    }

    #[inline]
    pub fn remove(&mut self, other: Ready) {
        self.0 &= !other.0;
    }

    #[deprecated(since = "0.6.5", note = "removed")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    #[inline]
    pub fn bits(&self) -> usize {
        self.0
    }

    #[inline]
    pub fn contains(&self, other: Ready) -> bool {
        (*self & other) == other
    }
}

impl ReadyUnix for Ready {
    #[inline]
    fn error() -> Self {
        Ready(ERROR)
    }

    #[inline]
    fn hup() -> Self {
        Ready(HUP)
    }

    #[inline]
    fn is_error(&self) -> bool {
        self.contains(Ready(ERROR))
    }

    #[inline]
    fn is_hup(&self) -> bool {
        self.contains(Ready(HUP))
    }
}

impl ops::BitOr for Ready {
    type Output = Ready;

    #[inline]
    fn bitor(self, other: Ready) -> Ready {
        Ready(self.0 | other.0)
    }
}

impl ops::BitXor for Ready {
    type Output = Ready;

    #[inline]
    fn bitxor(self, other: Ready) -> Ready {
        Ready(self.0 ^ other.0)
    }
}

impl ops::BitAnd for Ready {
    type Output = Ready;

    #[inline]
    fn bitand(self, other: Ready) -> Ready {
        Ready(self.0 & other.0)
    }
}

impl ops::Sub for Ready {
    type Output = Ready;

    #[inline]
    fn sub(self, other: Ready) -> Ready {
        Ready(self.0 & !other.0)
    }
}

impl ops::Not for Ready {
    type Output = Ready;

    #[inline]
    fn not(self) -> Ready {
        Ready(!self.0 & READY_ALL)
    }
}

impl fmt::Debug for Ready {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut one = false;
        let flags = [
            (Ready::readable(), "Readable"),
            (Ready::writable(), "Writable"),
            (Ready(ERROR), "Error"),
            (Ready(HUP), "Hup")];

        try!(write!(fmt, "Ready {{"));

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one { try!(write!(fmt, " | ")) }
                try!(write!(fmt, "{}", msg));

                one = true
            }
        }

        try!(write!(fmt, "}}"));

        Ok(())
    }
}

/// An readiness event returned by `Poll`.
///
/// Event represents the raw event that the OS-specific selector
/// returned. An event can represent more than one kind (such as
/// readable or writable) at a time.
///
/// These Event objects are created by the OS-specific concrete
/// Selector when they have events to report.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Event {
    kind: Ready,
    token: Token
}

impl Event {
    /// Create a new Event.
    pub fn new(kind: Ready, token: Token) -> Event {
        Event {
            kind: kind,
            token: token,
        }
    }

    pub fn readiness(&self) -> Ready {
        self.kind
    }

    #[deprecated(since = "0.6.5", note = "use Event::readiness()")]
    #[cfg(feature = "with-deprecated")]
    #[doc(hidden)]
    pub fn kind(&self) -> Ready {
        self.kind
    }

    pub fn token(&self) -> Token {
        self.token
    }
}

/*
 *
 * ===== Mio internal helpers =====
 *
 */

pub fn ready_as_usize(events: Ready) -> usize {
    events.0
}

pub fn opt_as_usize(opt: PollOpt) -> usize {
    opt.0
}

pub fn ready_from_usize(events: usize) -> Ready {
    Ready(events)
}

pub fn opt_from_usize(opt: usize) -> PollOpt {
    PollOpt(opt)
}

// Used internally to mutate an `Event` in place
// Not used on all platforms
#[allow(dead_code)]
pub fn kind_mut(event: &mut Event) -> &mut Ready {
    &mut event.kind
}
