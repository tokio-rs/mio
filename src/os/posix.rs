use {net, IoHandle, MioResult, MioError};
use std::mem;

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
    reader: IoDesc,
    writer: IoDesc
}

impl PipeAwakener {
    pub fn new() -> MioResult<PipeAwakener> {
        let (rd, wr) = try!(pipe());

        Ok(PipeAwakener {
            reader: rd,
            writer: wr
        })
    }

    pub fn wakeup(&self) -> MioResult<()> {
        net::write(&self.writer, b"0x01")
            .map(|_| ())
    }

    pub fn desc(&self) -> &IoDesc {
        &self.reader
    }

    pub fn cleanup(&self) {
        let mut buf: [u8; 128] = unsafe { mem::uninitialized() };

        loop {
            // Consume data until all bytes are purged
            match net::read(&self.reader, buf.as_mut_slice()) {
                Ok(_) => {}
                Err(_) => return
            }
        }
    }
}

/// Represents the OS's handle to the IO instance. In this case, it is the file
/// descriptor.
#[derive(Debug)]
pub struct IoDesc {
    pub fd: nix::Fd
}

impl IoHandle for IoDesc {
    fn desc(&self) -> &IoDesc {
        self
    }
}

impl Drop for IoDesc {
    fn drop(&mut self) {
        let _ = nix::close(self.fd);
    }
}

/*
 *
 * ===== Pipes =====
 *
 */

pub fn pipe() -> MioResult<(IoDesc, IoDesc)> {
    let (rd, wr) = try!(nix::pipe2(nix::O_NONBLOCK | nix::O_CLOEXEC)
                        .map_err(MioError::from_nix_error));

    Ok((IoDesc { fd: rd }, IoDesc { fd: wr }))
}
