use nix;

pub type MioResult<T> = Result<T, MioError>;
pub type MioError = nix::SysError;
