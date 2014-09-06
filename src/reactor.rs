use error::MioResult;
use token::Token;
use io::*;
use event::*;
use os;

/// A lightweight IO reactor.
///
/// An internal lookup structure is used to associate tokens with io
/// descriptors as well as track whether a socket is a listener or not.

#[deriving(Clone, Show)]
pub struct ReactorConfig;

pub struct Reactor<T> {
    selector: os::Selector,
}

impl<T: Token> Reactor<T> {
    /// Initializes a new reactor. The reactor will not be running yet.
    pub fn new() -> MioResult<Reactor<T>> {
        Ok(Reactor {
            selector: try!(os::Selector::new()),
        })
    }

    /// Registers an IO descriptor with the reactor.
    pub fn register<S: Socket>(&mut self, io: S, token: T, events: IoEventKind) -> MioResult<()> {
        debug!("registering IO with reactor");

        // Register interets for this socket
        try!(self.selector.register(io.desc(), token.to_u64(), events));

        Ok(())
    }


    pub fn run(&mut self, timeout: uint, handler: fn(token: T, event: IoEventKind) -> bool) {
        
        // Created here for stack allocation
        while self.io_poll(timeout, handler) {
            debug!("reactor tick");
        }

    }

    fn io_poll(&mut self, timeout: uint, handler: fn(token: T, event: IoEventKind) -> bool) -> bool {
        

        let len = self.selector.select(timeout).unwrap();
        let mut i = 0; 

        while i < len && i < self.selector.event_context.len() {
            let evt = self.selector.event_context[i].to_ioevent();
            let tok : T = Token::from_u64(self.selector.event_context[i].data);

            debug!("event={}", evt);

            if ! handler(tok, evt) {
               return false;
            }

            i += 1;
        }

        true
    }
}


#[cfg(test)]
mod tests {
    use std;
    use std::sync::Arc;
    use std::sync::atomics::{AtomicInt, SeqCst};

    use super::Reactor;
    use buf::{SliceBuf, MutSliceBuf};
    use io::{IoWriter, IoReader};
    use os;
    use event::*;

    struct Funtimes {
        readable: Arc<AtomicInt>,
        writable: Arc<AtomicInt>
    }

    impl Funtimes {
        fn new(readable: Arc<AtomicInt>, writable: Arc<AtomicInt>) -> Funtimes {
            Funtimes { readable: readable, writable: writable }
        }
    }

    #[test]
    fn test_readable() {
        let mut reactor = Reactor::<u64>::new().ok().expect("Couldn't make reactor");
        let pipe = unsafe { std::os::pipe() }.ok().expect("Couldn't create pipe");
        let mut reader = os::IoDesc { fd: pipe.reader };
        let mut writer = os::IoDesc { fd: pipe.writer };

        let mut buf = SliceBuf::wrap("hello".as_bytes());

        let read_count = Arc::new(AtomicInt::new(0));
        let write_count = Arc::new(AtomicInt::new(0));

        writer.write(&mut buf);

        reactor.register(reader, 10u64, IoReadable | IoWritable);
        
        let fun = Funtimes::new(read_count.clone(), write_count.clone());

        reactor.run(1000,  |tok: u64, event: IoEventKind| {
          if event.is_readable() {
            fun.fetch_add(1, SeqCst);
            assert_eq!(tok, 10u64);
          }
          false;
        });

        assert_eq!((*read_count).load(SeqCst), 1);

        let mut write_vec = vec![0u8, 0u8, 0u8, 0u8, 0u8];

        {
            let mut write_into = MutSliceBuf::wrap(write_vec.as_mut_slice());
            reader.read(&mut write_into).ok().expect("Couldn't read");
        }

        assert_eq!(String::from_utf8(write_vec).ok().expect("Invalid UTF-8").as_slice(), "hello");
    }
}
