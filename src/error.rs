use nix::errno::{SysError, EAGAIN};

pub type MioResult<T> = Result<T, MioError>;

#[deriving(Show, PartialEq, Clone)]
pub struct MioError {
    kind: MioErrorKind,
    sys: Option<SysError>
}

#[deriving(Show, PartialEq, Clone)]
pub enum MioErrorKind {
    Eof,
    WouldBlock,
    SysError,
}

impl MioError {
    pub fn eof() -> MioError {
        MioError {
            kind: Eof,
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
}
