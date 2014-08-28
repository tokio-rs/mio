use std::io;
use nix::errno::{SysError, EAGAIN};

pub type MioResult<T> = Result<T, MioError>;

#[deriving(Show, PartialEq, Clone)]
pub struct MioError {
    kind: MioErrorKind,
    sys: Option<SysError>
}

#[deriving(Show, PartialEq, Clone)]
pub enum MioErrorKind {
    Eof,          // End of file or socket closed
    SysError,     // System error not covered by other kinds
    WouldBlock,   // The operation would have blocked
    BufUnderflow, // Buf does not contain enough data to perform read op
    BufOverflow,  // Buf does not contain enough capacity to perform write op
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
            _ => SysError
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

    pub fn as_io_error(&self) -> io::IoError {
        match self.kind {
            Eof | BufUnderflow | BufOverflow => io::standard_error(io::EndOfFile),
            WouldBlock => io::standard_error(io::ResourceUnavailable),
            SysError => match self.sys {
                Some(err) => io::IoError::from_errno(err.kind as uint, false),
                None => io::standard_error(io::OtherIoError)
            }
        }
    }
}
