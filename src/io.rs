// Re-export the io::Result / Error types for convenience
pub use std::io::{Read, Write, Result, Error, ErrorKind};

// TODO: Delete this
/// A helper trait to provide the map_non_block function on Results.
pub trait MapNonBlock<T> {
    /// Maps a `Result<T>` to a `Result<Option<T>>` by converting
    /// operation-would-block errors into `Ok(None)`.
    fn map_non_block(self) -> Result<Option<T>>;
}

impl<T> MapNonBlock<T> for Result<T> {
    fn map_non_block(self) -> Result<Option<T>> {
        use std::io::ErrorKind::WouldBlock;

        match self {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                if let WouldBlock = err.kind() {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }
}

#[cfg(feature = "with-deprecated")]
pub mod deprecated {
    #[cfg(unix)]
    const WOULDBLOCK: i32 = ::libc::EAGAIN;

    #[cfg(windows)]
    const WOULDBLOCK: i32 = ::winapi::winerror::WSAEWOULDBLOCK as i32;

    /// Returns a std `WouldBlock` error without allocating
    pub fn would_block() -> ::std::io::Error {
        ::std::io::Error::from_raw_os_error(WOULDBLOCK)
    }
}

/*
 *
 * ===== UNIX ext =====
 *
 */

#[cfg(unix)]
use std::os::unix::io::RawFd;

#[cfg(unix)]
impl Evented for RawFd {
    fn register(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> Result<()> {
        selector.register(*self, token, interest, opts)
    }

    fn reregister(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> Result<()> {
        selector.reregister(*self, token, interest, opts)
    }

    fn deregister(&self, selector: &mut Selector) -> Result<()> {
        selector.deregister(*self)
    }
}
