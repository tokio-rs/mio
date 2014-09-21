use std::default::Default;
use std::u64;
use error::{MioResult, MioError};
use handler::{Handler, Token};
use io::{IoAcceptor, IoHandle};
use notify::Notify;
use os;
use poll::{Poll, IoEvent};
use socket::{Socket, SockAddr};

/// A lightweight IO reactor.
///
/// TODO:
/// - Enforce private tokens

#[deriving(Clone, Show)]
pub struct ReactorConfig {
    pub io_poll_timeout_ms: uint,

    // == Notifications ==
    pub notify_capacity: uint,
    pub messages_per_tick: uint
}

impl Default for ReactorConfig {
    fn default() -> ReactorConfig {
        ReactorConfig {
            io_poll_timeout_ms: 1_000,
            notify_capacity: 1024,
            messages_per_tick: 64,
        }
    }
}

pub struct Reactor<T, M: Send> {
    run: bool,
    poll: Poll,
    notify: Notify<M>,
    config: ReactorConfig,
}

// Token used to represent notifications
static NOTIFY: u64 = u64::MAX;

impl<T: Token, M: Send> Reactor<T, M> {
    /// Initializes a new reactor. The reactor will not be running yet.
    pub fn new() -> MioResult<Reactor<T, M>> {
        Reactor::configured(Default::default())
    }

    pub fn configured(config: ReactorConfig) -> MioResult<Reactor<T, M>> {
        let mut poll = try!(Poll::new());
        let notify = try!(Notify::with_capacity(config.notify_capacity));

        try!(poll.register(&notify, NOTIFY));

        Ok(Reactor {
            run: true,
            poll: poll,
            notify: notify,
            config: config,
        })
    }

    /// Returns a sender that allows sending messages to the reactor in a
    /// thread-safe way, waking up the reactor if needed.
    pub fn channel(&self) -> ReactorSender<M> {
        ReactorSender::new(self.notify.clone())
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
        self.poll.register(io, token.to_u64())
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
    pub fn run<H: Handler<T, M>>(&mut self, mut handler: H) -> ReactorResult<H> {
        self.run = true;

        while self.run {
            // Execute ticks as long as the reactor is running
            match self.tick(&mut handler) {
                Err(e) => return Err(ReactorError::new(handler, e)),
                _ => {}
            }
        }

        Ok(handler)
    }

    /// Spin the reactor once, with a timeout of one second, and notify the
    /// handler if any of the registered handles become ready during that
    /// time.
    pub fn run_once<H: Handler<T, M>>(&mut self, mut handler: H) -> ReactorResult<H> {
        // Execute a single tick
        match self.tick(&mut handler) {
            Err(e) => return Err(ReactorError::new(handler, e)),
            _ => {}
        }

        Ok(handler)
    }

    // Executes a single run of the reactor loop
    fn tick<H: Handler<T, M>>(&mut self, handler: &mut H) -> MioResult<()> {
        let mut messages;
        let mut pending;

        debug!("reactor tick");

        // Check the notify channel for any pending messages. If there are any,
        // avoid blocking when polling for IO events. Messages will be
        // processed after IO events.
        messages = self.notify.check(self.config.messages_per_tick, true);
        pending = messages > 0;

        // Check the registered IO handles for any new events. Each poll
        // is for one second, so a shutdown request can last as long as
        // one second before it takes effect.
        let events = try!(self.io_poll(pending));

        if !pending {
            // Indicate that the sleep period is over, also grab any additional
            // messages
            let remaining = self.config.messages_per_tick - messages;
            messages += self.notify.check(remaining, false);
        }

        self.io_process(handler, events);
        self.notify(handler, messages);

        Ok(())
    }

    #[inline]
    fn io_poll(&mut self, immediate: bool) -> MioResult<uint> {
        if immediate {
            self.poll.poll(0)
        } else {
            self.poll.poll(self.config.io_poll_timeout_ms)
        }
    }

    // Process IO events that have been previously polled
    fn io_process<H: Handler<T, M>>(&mut self, handler: &mut H, cnt: uint) {
        let mut i = 0u;

        // Iterate over the notifications. Each event provides the token
        // it was registered with (which usually represents, at least, the
        // handle that the event is about) as well as information about
        // what kind of event occurred (readable, writable, signal, etc.)
        while i < cnt {
            let evt = self.poll.event(i);

            debug!("event={}", evt);

            match evt.token() {
                NOTIFY => self.notify.cleanup(),
                _      => self.io_event(handler, evt)
            }

            i += 1;
        }
    }

    fn io_event<H: Handler<T, M>>(&mut self, handler: &mut H, evt: IoEvent) {
        let tok = Token::from_u64(evt.token());

        if evt.is_readable() {
            handler.readable(self, tok);
        }

        if evt.is_writable() {
            handler.writable(self, tok);
        }

        if evt.is_error() {
            println!(" + ERROR");
        }
    }

    fn notify<H: Handler<T, M>>(&mut self, handler: &mut H, mut cnt: uint) {
        while cnt > 0 {
            let msg = self.notify.poll()
                .expect("[BUG] at this point there should always be a message");

            handler.notify(self, msg);
            cnt -= 1;
        }
    }
}

#[deriving(Clone)]
pub struct ReactorSender<M: Send> {
    notify: Notify<M>
}

impl<M: Send> ReactorSender<M> {
    fn new(notify: Notify<M>) -> ReactorSender<M> {
        ReactorSender { notify: notify }
    }

    pub fn send(&self, msg: M) -> Result<(), M> {
        self.notify.notify(msg)
    }
}

pub type ReactorResult<H> = Result<H, ReactorError<H>>;

pub struct ReactorError<H> {
    pub handler: H,
    pub error: MioError
}

impl<H> ReactorError<H> {
    fn new(handler: H, error: MioError) -> ReactorError<H> {
        ReactorError {
            handler: handler,
            error: error
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str;
    use std::sync::Arc;
    use std::sync::atomics::{AtomicInt, SeqCst};
    use super::Reactor;
    use io::{IoWriter, IoReader};
    use {io, buf, Buf, Handler};

    type TestReactor = Reactor<u64, ()>;

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

    impl Handler<u64, ()> for Funtimes {
        fn readable(&mut self, _reactor: &mut TestReactor, token: u64) {
            (*self.rcount).fetch_add(1, SeqCst);
            assert_eq!(token, 10u64);
        }
    }

    #[test]
    fn test_readable() {
        let mut reactor = Reactor::new().ok().expect("Couldn't make reactor");

        let (mut reader, mut writer) = io::pipe().unwrap();

        let rcount = Arc::new(AtomicInt::new(0));
        let wcount = Arc::new(AtomicInt::new(0));
        let handler = Funtimes::new(rcount.clone(), wcount.clone());

        writer.write(&mut buf::wrap("hello".as_bytes())).unwrap();
        reactor.register(&reader, 10u64).unwrap();

        let _ = reactor.run_once(handler);
        let mut b = buf::ByteBuf::new(16);

        assert_eq!((*rcount).load(SeqCst), 1);

        reader.read(&mut b).unwrap();
        b.flip();

        assert_eq!(str::from_utf8(b.bytes()).unwrap(), "hello");
    }
}
