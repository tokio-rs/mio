use {Poll, EventSet, PollOpt, Token};

// Re-export the io::Result / Error types for convenience
pub use std::io::{Read, Write, Result, Error, ErrorKind};

/// A value that may be registered with an `EventLoop`
pub trait Evented {
    fn register(&self, poll: &Poll, token: Token, interest: EventSet, opts: PollOpt) -> Result<()>;

    fn reregister(&self, poll: &Poll, token: Token, interest: EventSet, opts: PollOpt) -> Result<()>;

    fn deregister(&self, poll: &Poll) -> Result<()>;
}

pub trait TryAccept {
    type Output;

    fn accept(&self) -> Result<Option<Self::Output>>;
}

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
