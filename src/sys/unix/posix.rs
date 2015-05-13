use {TryRead, TryWrite};
use io::{self, PipeReader, PipeWriter};
use std::mem;
use std::cell::UnsafeCell;
use std::os::unix::io::{RawFd, AsRawFd};

/*
 *
 * ===== Awakener =====
 *
 */

pub struct Awakener {
    reader: UnsafeCell<PipeReader>,
    writer: UnsafeCell<PipeWriter>,
}

impl Awakener {
    pub fn new() -> io::Result<Awakener> {
        let (rd, wr) = try!(io::pipe());

        Ok(Awakener {
            reader: UnsafeCell::new(rd),
            writer: UnsafeCell::new(wr),
        })
    }

    pub fn as_raw_fd(&self) -> RawFd {
        unsafe {
            let rd: &PipeReader = mem::transmute(self.reader.get());
            rd.as_raw_fd()
        }
    }

    pub fn wakeup(&self) -> io::Result<()> {
        // A hack, but such is life. PipeWriter is backed by a single FD, which
        // is thread safe.
        unsafe {
            let wr: &mut PipeWriter = mem::transmute(self.writer.get());
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
                let rd: &mut PipeReader = mem::transmute(self.reader.get());

                // Consume data until all bytes are purged
                match rd.read_slice(&mut buf) {
                    Ok(Some(i)) if i > 0 => {},
                    _ => return,
                }
            }
        }
    }
}
