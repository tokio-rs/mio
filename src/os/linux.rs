use {Io, NonBlock, TryRead, TryWrite, MioResult, MioError};
use std::os::unix::{Fd, AsRawFd};

const MARK: &'static [u8] = b"0x000x000x000x000x000x000x000x01";

mod nix {
    pub use nix::sys::eventfd::*;
}

pub struct Awakener {
    io: Io,
}

impl Awakener {
    pub fn new() -> MioResult<Awakener> {
        Ok(Awakener {
            io: Io::new(try!(eventfd())),
        })
    }

    pub fn wakeup(&self) -> MioResult<()> {
        self.io.write_slice(MARK)
            .map(|_| ())
    }

    pub fn as_raw_fd(&self) -> Fd {
        self.io.as_raw_fd()
    }

    pub fn cleanup(&self) {
        let mut buf = [0; 8];

        loop {
            // Consume data until all bytes are purged
            match self.io.read_slice(&mut buf) {
                Ok(NonBlock::Ready(i)) if i > 0 => {},
                _ => return,
            }
        }
    }
}

fn eventfd() -> MioResult<Fd> {
    nix::eventfd(0, nix::EFD_CLOEXEC | nix::EFD_NONBLOCK)
        .map_err(MioError::from_nix_error)
}
