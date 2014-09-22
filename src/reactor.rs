use error::{MioResult};
use handler;
use handler::Handler;
use io::{IoAcceptor, IoHandle};
use nix::fcntl::Fd;
use os;
use poll::Poll;
use socket::{Socket, SockAddr};
use std::collections::hashmap::HashMap;
use std::collections::MutableMap;
use std::default::Default;
use std::mem;

/// A lightweight IO reactor.
///
/// An internal lookup structure is used to associate tokens with io
/// descriptors as well as track whether a socket is a listener or not.

#[deriving(Clone, Show)]
pub struct ReactorConfig {
    // TODO(cgaebel): Replace with a timerfd, and keep track of timers in mio.
    pub io_poll_timeout_ms: uint
}

impl Default for ReactorConfig {
    fn default() -> ReactorConfig {
        ReactorConfig {
            io_poll_timeout_ms: 1_000
        }
    }
}

pub struct Reactor {
    config: ReactorConfig,
    poll:   Poll,
    run:    bool,
    // TODO(cgaebel): Remove this, and just have a global reactor.
    outstanding_handlers: HashMap<Fd, *mut ()>,
}

impl Drop for Reactor {
    fn drop(&mut self) {
        unsafe {
            let outstanding_handlers =
                mem::replace(&mut self.outstanding_handlers, HashMap::new());

            for boxed_handler in outstanding_handlers.values() {
                handler::drop_handler(self, *boxed_handler);
            }
        }
    }
}

impl Reactor {
    /// Initializes a new reactor. The reactor will not be running yet.
    pub fn new() -> MioResult<Reactor> {
        Reactor::configured(Default::default())
    }

    pub fn configured(config: ReactorConfig) -> MioResult<Reactor> {
        Ok(Reactor {
            config: config,
            poll: try!(Poll::new()),
            run: true,
            outstanding_handlers: HashMap::new(),
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
    pub fn register<H: Handler>(&mut self, fd: Fd, handler: H) -> MioResult<()> {
        unsafe {
            let boxed: Box<handler::BoxedHandler<H>> =
                box handler::BoxedHandler::new(fd, handler);
            let the_box_as_uint: uint = mem::transmute(boxed);
            // The box is now conveniently forgotten into an int.

            let the_box_as_ptr: *mut () = mem::transmute(the_box_as_uint);

            self.outstanding_handlers.insert(fd, the_box_as_ptr);

            // Register interests for this socket.
            match self.poll.register(fd, the_box_as_uint) {
                Ok(()) => {},
                Err(e) => {
                    let boxed: Box<handler::BoxedHandler<H>>
                        = mem::transmute(the_box_as_uint);
                    drop(boxed);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Care must be taken to correctly free the handler. This should only be
    /// called from `handler::handle`.
    ///
    /// This will not free the handler, and is therefore unsafe to call from
    /// anywhere except `handler.rs`.
    pub unsafe fn unregister_fd(&mut self, fd: Fd) -> MioResult<()> {
        // This might be false if we're dropping.
        self.outstanding_handlers.remove(&fd);
        self.poll.unregister(fd)
    }

    /// Connects the socket to the specified address. When the operation
    /// completes, the handler will be notified with the supplied token.
    ///
    /// The goal of this method is to ensure that the reactor will always
    /// notify about the connection, even if the connection happens
    /// immediately. Otherwise, every consumer of the reactor would have
    /// to worry about possibly-immediate connection.
    pub fn connect<S: Socket, H: Handler>(&mut self, io: S,
                              addr: &SockAddr, handler: |S| -> H) -> MioResult<()> {

        debug!("socket connect; addr={}", addr);

        // Attempt establishing the context. This may not complete immediately.
        if try!(os::connect(io.desc(), addr)) {
            // On some OSs, connecting to localhost succeeds immediately. In
            // this case, queue the writable callback for execution during the
            // next reactor tick.
            debug!("socket connected immediately; addr={}", addr);
        }

        // Register interest with socket on the reactor
        let fd = io.desc().fd;
        try!(self.register(fd, handler(io)));

        Ok(())
    }

    pub fn listen<S, A: IoHandle + IoAcceptor<S>, H: Handler>(&mut self, io: A, backlog: uint,
                                                  handler: |A| -> H) -> MioResult<()> {

        debug!("socket listen");

        // Start listening
        try!(os::listen(io.desc(), backlog));

        // Wait for connections
        let fd = io.desc().fd;

        try!(self.register(fd, handler(io)));

        Ok(())
    }

    /// Keep spinning the reactor indefinitely, and notify the handler whenever
    /// any of the registered handles are ready.
    pub fn run(&mut self) -> MioResult<()> {
        self.run = true;

        while self.run {
            // Execute ticks as long as the reactor is running
            try!(self.tick())
        }

        Ok(())
    }

    /// Spin the reactor once, with a timeout of one second, and notify the
    /// handler if any of the registered handles become ready during that
    /// time.
    pub fn run_once(&mut self) -> MioResult<()> {
        // Execute a single tick
        self.tick()
    }

    // Executes a single run of the reactor loop
    fn tick(&mut self) -> MioResult<()> {
        debug!("reactor tick");

        // Check the registered IO handles for any new events. Each poll
        // is for one second, so a shutdown request can last as long as
        // one second before it takes effect.
        try!(self.io_poll());

        Ok(())
    }

    /// Poll the reactor for one second, calling the handler if any
    /// of the registered handles are ready.
    fn io_poll(&mut self) -> MioResult<()> {
        let cnt = try!(self.poll.poll(self.config.io_poll_timeout_ms));

        let mut i = 0u;

        // Iterate over the notifications. Each event provides the token
        // it was registered with (which usually represents, at least, the
        // handle that the event is about) as well as information about
        // what kind of event occurred (readable, writable, signal, etc.)
        while i < cnt {
            let evt = self.poll.event(i);

            let tok: uint = evt.token();

            debug!("processing tok: {}", tok);

            if tok != 0 {
                unsafe {
                    let boxed_handler: *mut () = mem::transmute(tok);
                    handler::call_handler(self, &evt, boxed_handler);
                }
            }

            i += 1;
        }

        debug!("{} handlers left.", self.outstanding_handlers.len());

        if self.outstanding_handlers.is_empty() {
            self.shutdown();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use error::MioResult;
    use iobuf::{RWIobuf, ROIobuf, Iobuf};
    use std::sync::Arc;
    use std::sync::atomics::{AtomicInt, SeqCst};
    use super::Reactor;
    use io::{IoWriter, IoReader, IoHandle};
    use {io, Handler};

    struct Funtimes {
        rcount: Arc<AtomicInt>,
        wcount: Arc<AtomicInt>
    }

    impl Funtimes {
        fn new(rcount: Arc<AtomicInt>, wcount: Arc<AtomicInt>) -> Funtimes {
            Funtimes {
                rcount: rcount,
                wcount: wcount
            }
        }
    }

    impl Handler for Funtimes {
        fn readable(&mut self, _reactor: &mut Reactor) -> MioResult<()> {
            (*self.rcount).fetch_add(1, SeqCst);
            Ok(())
        }

        fn writable(&mut self, _reactor: &mut Reactor) -> MioResult<()> { Ok(()) }
    }

    #[test]
    fn test_readable() {
        let mut reactor = Reactor::new().ok().expect("Couldn't make reactor");

        let (reader, writer) = io::pipe().unwrap();

        let rcount = Arc::new(AtomicInt::new(0));
        let wcount = Arc::new(AtomicInt::new(0));
        let handler = Funtimes::new(rcount.clone(), wcount.clone());

        writer.write(&mut ROIobuf::from_str("hello")).unwrap();
        reactor.register(reader.desc().fd, handler).unwrap();

        let _ = reactor.run_once();
        let mut b = RWIobuf::new(16);

        assert_eq!((*rcount).load(SeqCst), 1);

        reader.read(&mut b).unwrap();
        b.flip_lo();

        unsafe { assert_eq!(b.as_slice(), b"hello"); }
    }
}
