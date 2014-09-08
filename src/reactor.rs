use error::{MioResult, MioError};
use handler::{Handler, ReadHint, DataHint, HupHint, UnknownHint, Token};
use io::*;
use os;

/// A lightweight IO reactor.
///
/// An internal lookup structure is used to associate tokens with io
/// descriptors as well as track whether a socket is a listener or not.

#[deriving(Clone, Show)]
pub struct ReactorConfig;

pub struct Reactor<T> {
    selector: os::Selector,
    run: bool
}

pub struct ReactorError<H> {
    handler: H,
    error: MioError
}

pub type ReactorResult<H> = Result<H, ReactorError<H>>;

impl<T: Token> Reactor<T> {
    /// Initializes a new reactor. The reactor will not be running yet.
    pub fn new() -> MioResult<Reactor<T>> {
        Ok(Reactor {
            selector: try!(os::Selector::new()),
            run: true
        })
    }

    /// Tells the reactor to exit after it is done handling all events in the
    /// current iteration.
    pub fn shutdown(&mut self) {
        self.run = false;
    }

    /// Tells the reactor to exit immidiately. All pending events will be dropped.
    pub fn shutdown_now(&mut self) {
        unimplemented!()
    }

    /// Registers an IO handle with the reactor.
    pub fn register<H: IoHandle>(&mut self, io: &H, token: T) -> MioResult<()> {
        debug!("registering IO with reactor");

        // Register interets for this socket
        try!(self.selector.register(io.desc(), token.to_u64()));

        Ok(())
    }

    /// Connects the socket to the specified address. When the operation
    /// completes, the handler will be notified with the supplied token.
    ///
    /// The goal of this method is to ensure that the reactor will always
    /// notify about the connection, even if the connection happens
    /// immediately. Otherwise, every consumer of the reactor would have
    /// to worry about possibly-immediate connection.
    pub fn connect<S: Socket>(&mut self, io: &S,
                              addr: &SockAddr, token: T) -> MioResult<()> {

        debug!("socket connect; addr={}", addr);

        // Attempt establishing the context. This may not complete immediately.
        if try!(os::connect(io.desc(), addr)) {
            // On some OSs, connecting to localhost succeeds immediately. In
            // this case, queue the writable callback for execution during the
            // next reactor tick.
            debug!("socket connected immediately; addr={}", addr);
        }

        // Register interest with socket on the reactor
        try!(self.register(io, token));

        Ok(())
    }

    pub fn listen<S, A: IoHandle + IoAcceptor<S>>(&mut self, io: &A, backlog: uint,
                                                  token: T) -> MioResult<()> {

        debug!("socket listen");

        // Start listening
        try!(os::listen(io.desc(), backlog));

        // Wait for connections
        try!(self.register(io, token));

        Ok(())
    }

    /// Keep spinning the reactor indefinitely, and notify the handler whenever
    /// any of the registered handles are ready.
    pub fn run<H: Handler<T>>(&mut self, mut handler: H) -> ReactorResult<H> {
        self.run = true;

        // Created here for stack allocation
        let mut events = os::Events::new();

        while self.run {
            debug!("reactor tick");

            // Check the registered IO handles for any new events. Each poll
            // is for one second, so a shutdown request can last as long as
            // one second before it takes effect.
            self.io_poll(&mut events, &mut handler);
        }

        Ok(handler)
    }

    /// Spin the reactor once, with a timeout of one second, and notify the
    /// handler if any of the registered handles become ready during that
    /// time.
    pub fn run_once<H: Handler<T>>(&mut self, mut handler: H) -> ReactorResult<H> {
        // Created here for stack allocation
        let mut events = os::Events::new();

        // Check the registered IO handles for any new events. Each poll
        // is for one second, so a shutdown request can last as long as
        // one second before it takes effect.
        self.io_poll(&mut events, &mut handler);

        Ok(handler)
    }

    /// Poll the reactor for one second, calling the handler if any
    /// of the registered handles are ready.
    fn io_poll<H: Handler<T>>(&mut self, events: &mut os::Events, handler: &mut H) {
        self.selector.select(events, 1000).unwrap();

        let mut i = 0u;

        // Iterate over the notifications. Each event provides the token
        // it was registered with (which usually represents, at least, the
        // handle that the event is about) as well as information about
        // what kind of event occurred (readable, writable, signal, etc.)
        while i < events.len() {
            let evt = events.get(i);
            let tok = Token::from_u64(evt.token);

            debug!("event={}", evt);

            if evt.is_readable() {
                handler.readable(self, tok, evt.read_hint());
            }

            if evt.is_writable() {
                handler.writable(self, tok);
            }

            if evt.is_error() {
                println!(" + ERROR");
            }

            i += 1;
        }
    }
}

bitflags!(
    #[deriving(Show)]
    flags IoEventKind: uint {
        static IoReadable = 0x001,
        static IoWritable = 0x002,
        static IoError    = 0x004,
        static IoHupHint  = 0x008,
        static IoHinted   = 0x010
    }
)

#[deriving(Show)]
pub struct IoEvent {
    kind: IoEventKind,
    token: u64
}

/// IoEvent represents the raw event that the OS-specific selector
/// returned. An event can represent more than one kind (such as
/// readable or writable) at a time.
///
/// These IoEvent objects are created by the OS-specific concrete
/// Selector when they have events to report.
impl IoEvent {
    /// Create a new IoEvent.
    pub fn new(kind: IoEventKind, token: u64) -> IoEvent {
        IoEvent {
            kind: kind,
            token: token
        }
    }

    /// Return an optional hint for a readable IO handle. Currently,
    /// this method supports the HupHint, which indicates that the
    /// kernel reported that the remote side hung up. This allows a
    /// consumer to avoid reading in order to discover the hangup.
    pub fn read_hint(&self) -> ReadHint {
        if self.kind.contains(IoHupHint) {
            HupHint
        } else if self.kind.contains(IoHinted) {
            DataHint
        } else {
            UnknownHint
        }
    }

    /// This event indicated that the IO handle is now readable
    pub fn is_readable(&self) -> bool {
        self.kind.contains(IoReadable) || self.kind.contains(IoHupHint)
    }

    /// This event indicated that the IO handle is now writable
    pub fn is_writable(&self) -> bool {
        self.kind.contains(IoWritable)
    }

    /// This event indicated that the IO handle had an error
    pub fn is_error(&self) -> bool {
        self.kind.contains(IoError)
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
    use handler::ReadHint;
    use Handler;
    use os;

    struct Funtimes {
        readable: Arc<AtomicInt>,
        writable: Arc<AtomicInt>
    }

    impl Funtimes {
        fn new(readable: Arc<AtomicInt>, writable: Arc<AtomicInt>) -> Funtimes {
            Funtimes { readable: readable, writable: writable }
        }
    }

    impl Handler<u64> for Funtimes {
        fn readable(&mut self, _reactor: &mut Reactor<u64>, token: u64, _hint: ReadHint) {
            (*self.readable).fetch_add(1, SeqCst);
            assert_eq!(token, 10u64);
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

        writer.write(&mut buf).unwrap();

        reactor.register(&reader, 10u64).unwrap();
        let _ = reactor.run_once(Funtimes::new(read_count.clone(), write_count.clone()));

        assert_eq!((*read_count).load(SeqCst), 1);

        let mut write_vec = vec![0u8, 0u8, 0u8, 0u8, 0u8];

        {
            let mut write_into = MutSliceBuf::wrap(write_vec.as_mut_slice());
            reader.read(&mut write_into).ok().expect("Couldn't read");
        }

        assert_eq!(String::from_utf8(write_vec).ok().expect("Invalid UTF-8").as_slice(), "hello");
    }
}
