pub use self::pipe::Awakener;

/// Default *nix awakener implementation
mod pipe {
    use {io, Evented, EventSet, PollOpt, Selector, Token, TryRead, TryWrite};
    use unix::{self, PipeReader, PipeWriter};

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
            let (rd, wr) = try!(unix::pipe());

            Ok(Awakener {
                reader: rd,
                writer: wr,
            })
        }

        pub fn wakeup(&self) -> io::Result<()> {
            (&self.writer).try_write(b"0x01").map(|_| ())
        }

        pub fn cleanup(&self) {
            let mut buf = [0; 128];

            loop {
                // Consume data until all bytes are purged
                match (&self.reader).try_read(&mut buf) {
                    Ok(Some(i)) if i > 0 => {},
                    _ => return,
                }
            }
        }

        fn reader(&self) -> &PipeReader {
            &self.reader
        }
    }

    impl Evented for Awakener {
        fn register(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
            self.reader().register(selector, token, interest, opts)
        }

        fn reregister(&self, selector: &mut Selector, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
            self.reader().reregister(selector, token, interest, opts)
        }

        fn deregister(&self, selector: &mut Selector) -> io::Result<()> {
            self.reader().deregister(selector)
        }
    }
}

/*

TODO: Bring back eventfd awakener.
      Blocked on carllerche/nix-rust#98
mod eventfd {
    use {io, Io, TryRead, TryWrite};
    use std::mem;
    use std::os::unix::io::{RawFd, AsRawFd};

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
            .map_err(super::from_nix_error)
    }
}
 */
