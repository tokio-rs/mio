pub use self::pipe::Awakener;

/// Default *nix awakener implementation
mod pipe {
    use {io, Evented, EventSet, Poll, PollOpt, Token, TryRead, TryWrite};
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
        fn register(&self, poll: &mut Poll, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
            self.reader().register(poll, token, interest, opts)
        }

        fn reregister(&self, poll: &mut Poll, token: Token, interest: EventSet, opts: PollOpt) -> io::Result<()> {
            self.reader().reregister(poll, token, interest, opts)
        }

        fn deregister(&self, poll: &mut Poll) -> io::Result<()> {
            self.reader().deregister(poll)
        }
    }
}
