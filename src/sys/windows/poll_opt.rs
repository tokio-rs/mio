use std::{fmt, ops};

/// Options supplied when registering an `event::Source`s handle with `Poll`.
///
/// `PollOpt` values can be combined together using the various bitwise
/// operators.
///
/// For high level documentation on polling and poll options, see [`Poll`].
///
/// [`Poll`]: struct.Poll.html
#[derive(Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub(crate) struct PollOpt(usize);

impl PollOpt {
    /// Return a `PollOpt` representing no set options.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn empty() -> PollOpt {
        PollOpt(0)
    }

    /// Return a `PollOpt` representing edge-triggered notifications.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn edge() -> PollOpt {
        PollOpt(0b0001)
    }

    /// Return a `PollOpt` representing level-triggered notifications.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn level() -> PollOpt {
        PollOpt(0b0010)
    }

    /// Return a `PollOpt` representing oneshot notifications.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn oneshot() -> PollOpt {
        PollOpt(0b0100)
    }

    /// Returns true if the options include edge-triggered notifications.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_edge(&self) -> bool {
        self.contains(PollOpt::edge())
    }

    /// Returns true if the options includes oneshot.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn is_oneshot(&self) -> bool {
        self.contains(PollOpt::oneshot())
    }

    /// Returns true if `self` is a superset of `other`.
    ///
    /// `other` may represent more than one option, in which case the function
    /// only returns true if `self` contains all of the options specified in
    /// `other`.
    ///
    /// See [`Poll`] for more documentation on polling.
    ///
    /// [`Poll`]: struct.Poll.html
    #[inline]
    pub fn contains(&self, other: PollOpt) -> bool {
        (*self & other) == other
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

impl fmt::Debug for PollOpt {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut one = false;
        let flags = [
            (PollOpt::edge(), "Edge-Triggered"),
            (PollOpt::level(), "Level-Triggered"),
            (PollOpt::oneshot(), "OneShot"),
        ];

        for &(flag, msg) in &flags {
            if self.contains(flag) {
                if one {
                    write!(fmt, " | ")?
                }
                write!(fmt, "{}", msg)?;

                one = true
            }
        }

        if !one {
            fmt.write_str("(empty)")?;
        }

        Ok(())
    }
}

impl From<PollOpt> for usize {
    fn from(src: PollOpt) -> usize {
        src.0
    }
}

impl From<usize> for PollOpt {
    fn from(src: usize) -> PollOpt {
        PollOpt(src)
    }
}

#[test]
fn test_debug_pollopt() {
    assert_eq!("(empty)", format!("{:?}", PollOpt::empty()));
    assert_eq!("Edge-Triggered", format!("{:?}", PollOpt::edge()));
    assert_eq!("Level-Triggered", format!("{:?}", PollOpt::level()));
    assert_eq!("OneShot", format!("{:?}", PollOpt::oneshot()));
}
