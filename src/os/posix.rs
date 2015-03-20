use {TryRead, TryWrite};
use io::{self, PipeReader, PipeWriter};
use std::mem;
use std::os::unix::io::{Fd, AsRawFd};

/*
 *
 * ===== Awakener =====
 *
 */

pub struct Awakener {
    reader: PipeReader,
    writer: PipeWriter,
}

impl Awakener {
    pub fn new() -> io::Result<Awakener> {
        let (rd, wr) = try!(io::pipe());

        Ok(Awakener {
            reader: rd,
            writer: wr
        })
    }

    pub fn as_raw_fd(&self) -> Fd {
        self.reader.as_raw_fd()
    }

    pub fn wakeup(&self) -> io::Result<()> {
        // A hack, but such is life. PipeWriter is backed by a single FD, which
        // is thread safe.
        unsafe {
            let wr: &mut PipeWriter = mem::transmute(&self.writer);
            wr.write_slice(b"0x01").map(|_| ())
        }
    }

    pub fn cleanup(&self) {
        let mut buf = [0; 128];

        loop {
            // Also a bit hackish. It would be possible to split up the read /
            // write sides of the awakener, but that would be a more
            // significant refactor. A transmute here is safe.
            unsafe {
                let rd: &mut PipeReader = mem::transmute(&self.reader);

                // Consume data until all bytes are purged
                match rd.read_slice(&mut buf) {
                    Ok(Some(i)) if i > 0 => {},
                    _ => return,
                }
            }
        }
    }
}
