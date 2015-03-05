use {io, Io, TryRead, TryWrite};
use std::mem;
use std::os::unix::{Fd, AsRawFd};

const MARK: &'static [u8] = b"0x000x000x000x000x000x000x000x01";

mod nix {
    pub use nix::sys::eventfd::*;
}

pub struct Awakener {
    io: Io,
}

impl Awakener {
    pub fn new() -> io::Result<Awakener> {
        Ok(Awakener {
            io: Io::new(try!(eventfd())),
        })
    }

    pub fn wakeup(&self) -> io::Result<()> {
        unsafe {
            let io: &mut Io = mem::transmute(&self.io);

            io.write_slice(MARK)
                .map(|_| ())
        }
    }

    pub fn as_raw_fd(&self) -> Fd {
        self.io.as_raw_fd()
    }

    pub fn cleanup(&self) {
        let mut buf = [0; 8];

        loop {
            unsafe {
                let io: &mut Io = mem::transmute(&self.io);

                // Consume data until all bytes are purged
                match io.read_slice(&mut buf) {
                    Ok(Some(i)) if i > 0 => {},
                    _ => return,
                }
            }
        }
    }
}

fn eventfd() -> io::Result<Fd> {
    nix::eventfd(0, nix::EFD_CLOEXEC | nix::EFD_NONBLOCK)
        .map_err(io::from_nix_error)
}
