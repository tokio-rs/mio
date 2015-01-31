use std::old_io;
use nix::errno::{SysError, EAGAIN, EADDRINUSE};

use self::MioErrorKind::{
    Eof,
    BufUnderflow,
    BufOverflow,
    WouldBlock,
    AddrInUse,
    EventLoopTerminated,
    OtherError
};

pub type MioResult<T> = Result<T, MioError>;

#[derive(Copy, Debug, PartialEq, Clone)]
pub struct MioError {
    pub kind: MioErrorKind,
    sys: Option<SysError>
}

#[derive(Copy, Debug, PartialEq, Clone)]
pub enum MioErrorKind {
    Eof,                    // End of file or socket closed
    WouldBlock,             // The operation would have blocked
    AddrInUse,              // Inet socket address or domain socket path already in use
    BufUnderflow,           // Buf does not contain enough data to perform read op
    BufOverflow,            // Buf does not contain enough capacity to perform write op
    EventLoopTerminated,    // The event loop is not running anymore
    OtherError,             // System error not covered by other kinds
}

impl MioError {
    pub fn eof() -> MioError {
        MioError {
            kind: Eof,
            sys: None
        }
    }

    pub fn buf_underflow() -> MioError {
        MioError {
            kind: BufUnderflow,
            sys: None
        }
    }

    pub fn buf_overflow() -> MioError {
        MioError {
            kind: BufOverflow,
            sys: None
        }
    }

    pub fn from_sys_error(err: SysError) -> MioError {
        let kind = match err.kind {
            EAGAIN => WouldBlock,
            EADDRINUSE => AddrInUse,
            _ => OtherError
        };

        MioError {
            kind: kind,
            sys: Some(err)
        }
    }

    pub fn is_eof(&self) -> bool {
        match self.kind {
            Eof => true,
            _ => false
        }
    }

    pub fn is_would_block(&self) -> bool {
        match self.kind {
            WouldBlock => true,
            _ => false
        }
    }

    pub fn is_buf_underflow(&self) -> bool {
        match self.kind {
            BufUnderflow => true,
            _ => false
        }
    }

    pub fn is_buf_overflow(&self) -> bool {
        match self.kind {
            BufOverflow => true,
            _ => false
        }
    }

    pub fn as_io_error(&self) -> old_io::IoError {
        use std::old_io::OtherIoError;

        match self.kind {
            Eof | BufUnderflow | BufOverflow => old_io::standard_error(old_io::EndOfFile),
            WouldBlock => old_io::standard_error(old_io::ResourceUnavailable),
            AddrInUse => old_io::standard_error(old_io::PathAlreadyExists),
            OtherError => match self.sys {
                Some(err) => old_io::IoError::from_errno(err.kind as usize, false),
                None => old_io::standard_error(old_io::OtherIoError)
            },
            EventLoopTerminated => old_io::standard_error(OtherIoError)
        }
    }
}
