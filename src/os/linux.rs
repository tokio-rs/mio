use std::mem;
use super::posix::*;
use error::{MioResult, MioError};

const MARK: &'static [u8] = b"0x000x000x000x000x000x000x000x01";

mod nix {
    pub use nix::sys::eventfd::*;
}

pub struct Awakener {
    eventfd: IoDesc
}

impl Awakener {
    pub fn new() -> MioResult<Awakener> {
        Ok(Awakener { eventfd: try!(eventfd()) })
    }

    pub fn wakeup(&self) -> MioResult<()> {
        write(&self.eventfd, MARK)
            .map(|_| ())
    }

    pub fn desc(&self) -> &IoDesc {
        &self.eventfd
    }

    pub fn cleanup(&self) {
        let mut buf: [u8; 8] = unsafe { mem::uninitialized() };

        loop {
            // Consume data until all bytes are purged
            match read(&self.eventfd, buf.as_mut_slice()) {
                Ok(_) => {}
                Err(_) => return
            }
        }
    }
}

fn eventfd() -> MioResult<IoDesc> {
    let fd = try!(nix::eventfd(0, nix::EFD_CLOEXEC | nix::EFD_NONBLOCK)
                    .map_err(MioError::from_nix_error));

    Ok(IoDesc { fd: fd })
}
