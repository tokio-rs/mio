use {IoHandle, TryRead, TryWrite, NonBlock, MioResult};
use io::{self, PipeReader, PipeWriter};
use std::os::unix::Fd;

mod nix {
    pub use nix::{c_int, NixError};
    pub use nix::fcntl::{Fd, O_NONBLOCK, O_CLOEXEC};
    pub use nix::errno::EINPROGRESS;
    pub use nix::sys::socket::*;
    pub use nix::unistd::*;
}

/*
 *
 * ===== Awakener =====
 *
 */

pub struct PipeAwakener {
    reader: PipeReader,
    writer: PipeWriter,
}

impl PipeAwakener {
    pub fn new() -> MioResult<PipeAwakener> {
        let (rd, wr) = try!(io::pipe());

        Ok(PipeAwakener {
            reader: rd,
            writer: wr
        })
    }

    pub fn fd(&self) -> Fd {
        self.reader.fd()
    }

    pub fn wakeup(&self) -> MioResult<()> {
        self.writer.write_slice(b"0x01").map(|_| ())
    }

    pub fn cleanup(&self) {
        let mut buf = [0; 128];

        loop {
            // Consume data until all bytes are purged
            match self.reader.read_slice(&mut buf) {
                Ok(NonBlock::Ready(i)) if i > 0 => {},
                _ => return,
            }
        }
    }
}
