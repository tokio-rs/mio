use {Io, NonBlock, IoReader, IoWriter, IoHandle, MioResult, MioError};
use super::posix::*;

const MARK: &'static [u8] = b"0x000x000x000x000x000x000x000x01";

mod nix {
    pub use nix::sys::eventfd::*;
}

pub struct Awakener {
    eventfd: Io,
}

impl Awakener {
    pub fn new() -> MioResult<Awakener> {
        Ok(Awakener {
            eventfd: Io::new(try!(eventfd())),
        })
    }

    pub fn wakeup(&self) -> MioResult<()> {
        self.eventfd.write_slice(MARK)
            .map(|_| ())
    }

    pub fn desc(&self) -> &IoDesc {
        self.eventfd.desc()
    }

    pub fn cleanup(&self) {
        let mut buf = [0; 8];

        loop {
            // Consume data until all bytes are purged
            match self.eventfd.read_slice(&mut buf) {
                Ok(NonBlock::Ready(i)) if i > 0 => {},
                _ => return,
            }
        }
    }
}

fn eventfd() -> MioResult<IoDesc> {
    let fd = try!(nix::eventfd(0, nix::EFD_CLOEXEC | nix::EFD_NONBLOCK)
                    .map_err(MioError::from_nix_error));

    Ok(IoDesc { fd: fd })
}
