use std::default::Default;
use std::uint;
use error::{MioResult, MioError};
use handler::Handler;
use io::{IoAcceptor, IoHandle};
use notify::Notify;
use os;
use poll::{Poll, IoEvent};
use socket::{Socket, SockAddr};
use timer::{Timer, Timeout, TimerResult};
use token::Token;

/// A lightweight event loop.
///
/// TODO:
/// - Enforce private tokens

#[deriving(Clone, Show)]
pub struct EventLoopConfig {
    pub io_poll_timeout_ms: uint,

    // == Notifications ==
    pub notify_capacity: uint,
    pub messages_per_tick: uint,

    // == Timer ==
    pub timer_tick_ms: u64,
    pub timer_wheel_size: uint,
    pub timer_capacity: uint,
}

impl Default for EventLoopConfig {
    fn default() -> EventLoopConfig {
        EventLoopConfig {
            io_poll_timeout_ms: 1_000,
            notify_capacity: 1_024,
            messages_per_tick: 64,
            timer_tick_ms: 100,
            timer_wheel_size: 1_024,
            timer_capacity: 65_536,
        }
    }
}

pub struct EventLoop<T, M: Send> {
    run: bool,
    poll: Poll,
    timer: Timer<T>,
    notify: Notify<M>,
    config: EventLoopConfig,
}

// Token used to represent notifications
static NOTIFY: Token = Token(uint::MAX);

impl<T, M: Send> EventLoop<T, M> {
    /// Initializes a new event loop. The event loop will not be running yet.
    pub fn new() -> MioResult<EventLoop<T, M>> {
        EventLoop::configured(Default::default())
    }

    pub fn configured(config: EventLoopConfig) -> MioResult<EventLoop<T, M>> {
        // Create the IO poller
        let mut poll = try!(Poll::new());

        // Create the timer
        let mut timer = Timer::new(
            config.timer_tick_ms,
            config.timer_wheel_size,
            config.timer_capacity);

        // Create cross thread notification queue
        let notify = try!(Notify::with_capacity(config.notify_capacity));

        // Register the notification wakeup FD with the IO poller
        try!(poll.register(&notify, NOTIFY));

        // Set the timer's starting time reference point
        timer.setup();

        Ok(EventLoop {
            run: true,
            poll: poll,
            timer: timer,
            notify: notify,
            config: config,
        })
    }

    /// Returns a sender that allows sending messages to the event loop in a
    /// thread-safe way, waking up the event loop if needed.
    pub fn channel(&self) -> EventLoopSender<M> {
        EventLoopSender::new(self.notify.clone())
    }

    /// After the requested time interval, the handler's `timeout` function
    /// will be called with the supplied token.
    pub fn timeout_ms(&mut self, token: T, delay: u64) -> TimerResult<Timeout> {
        self.timer.timeout_ms(token, delay)
    }

    /// If the supplied timeout has not been triggered, cancel it such that it
    /// will not be triggered in the future.
    pub fn clear_timeout(&mut self, timeout: Timeout) -> bool {
        self.timer.clear(timeout)
    }

    /// Tells the event loop to exit after it is done handling all events in the
    /// current iteration.
    pub fn shutdown(&mut self) {
        self.run = false;
    }

    /// Tells the event loop to exit immidiately. All pending events will be dropped.
    pub fn shutdown_now(&mut self) {
        unimplemented!()
    }

    /// Registers an IO handle with the event loop.
    pub fn register<H: IoHandle>(&mut self, io: &H, token: Token) -> MioResult<()> {
        self.poll.register(io, token)
    }

    /// Connects the socket to the specified address. When the operation
    /// completes, the handler will be notified with the supplied token.
    ///
    /// The goal of this method is to ensure that the event loop will always
    /// notify about the connection, even if the connection happens
    /// immediately. Otherwise, every consumer of the event loop would have
    /// to worry about possibly-immediate connection.
    pub fn connect<S: Socket>(&mut self, io: &S, addr: &SockAddr, token: Token) -> MioResult<()> {
        debug!("socket connect; addr={}", addr);

        // Attempt establishing the context. This may not complete immediately.
        if try!(os::connect(io.desc(), addr)) {
            // On some OSs, connecting to localhost succeeds immediately. In
            // this case, queue the writable callback for execution during the
            // next event loop tick.
            debug!("socket connected immediately; addr={}", addr);
        }

        // Register interest with socket on the event loop
        try!(self.register(io, token));

        Ok(())
    }

    pub fn listen<S, A: IoHandle + IoAcceptor<S>>(
        &mut self, io: &A, backlog: uint, token: Token) -> MioResult<()> {

        debug!("socket listen");

        // Start listening
        try!(os::listen(io.desc(), backlog));

        // Wait for connections
        try!(self.register(io, token));

        Ok(())
    }

    /// Keep spinning the event loop indefinitely, and notify the handler whenever
    /// any of the registered handles are ready.
    pub fn run<H: Handler<T, M>>(&mut self, mut handler: H) -> EventLoopResult<H> {
        self.run = true;

        while self.run {
            // Execute ticks as long as the event loop is running
            match self.tick(&mut handler) {
                Err(e) => return Err(EventLoopError::new(handler, e)),
                _ => {}
            }
        }

        Ok(handler)
    }

    /// Spin the event loop once, with a timeout of one second, and notify the
    /// handler if any of the registered handles become ready during that
    /// time.
    pub fn run_once<H: Handler<T, M>>(&mut self, mut handler: H) -> EventLoopResult<H> {
        // Execute a single tick
        match self.tick(&mut handler) {
            Err(e) => return Err(EventLoopError::new(handler, e)),
            _ => {}
        }

        Ok(handler)
    }

    // Executes a single run of the event loop loop
    fn tick<H: Handler<T, M>>(&mut self, handler: &mut H) -> MioResult<()> {
        let mut messages;
        let mut pending;

        debug!("event loop tick");

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
        self.timer_process(handler);

        Ok(())
    }

    #[inline]
    fn io_poll(&mut self, immediate: bool) -> MioResult<uint> {
        if immediate {
            self.poll.poll(0)
        } else {
            let mut sleep = self.timer.next_tick_in_ms() as uint;

            if sleep > self.config.io_poll_timeout_ms {
                sleep = self.config.io_poll_timeout_ms;
            }

            self.poll.poll(sleep)
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
        let tok = evt.token();

        if evt.is_readable() {
            handler.readable(self, tok, evt.read_hint());
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

    fn timer_process<H: Handler<T, M>>(&mut self, handler: &mut H) {
        let now = self.timer.now();

        loop {
            match self.timer.tick_to(now) {
                Some(t) => handler.timeout(self, t),
                _ => return
            }
        }
    }
}

#[deriving(Clone)]
pub struct EventLoopSender<M: Send> {
    notify: Notify<M>
}

impl<M: Send> EventLoopSender<M> {
    fn new(notify: Notify<M>) -> EventLoopSender<M> {
        EventLoopSender { notify: notify }
    }

    pub fn send(&self, msg: M) -> Result<(), M> {
        self.notify.notify(msg)
    }
}

pub type EventLoopResult<H> = Result<H, EventLoopError<H>>;

pub struct EventLoopError<H> {
    pub handler: H,
    pub error: MioError
}

impl<H> EventLoopError<H> {
    fn new(handler: H, error: MioError) -> EventLoopError<H> {
        EventLoopError {
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
    use super::EventLoop;
    use io::{IoWriter, IoReader};
    use {io, buf, Buf, Handler, Token, ReadHint};

    type TestEventLoop = EventLoop<uint, ()>;

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

    impl Handler<uint, ()> for Funtimes {
        fn readable(&mut self, _event_loop: &mut TestEventLoop, token: Token, hint: ReadHint) {
            (*self.rcount).fetch_add(1, SeqCst);
            assert_eq!(token, Token(10));
        }
    }

    #[test]
    fn test_readable() {
        let mut event_loop = EventLoop::new().ok().expect("Couldn't make event loop");

        let (mut reader, mut writer) = io::pipe().unwrap();

        let rcount = Arc::new(AtomicInt::new(0));
        let wcount = Arc::new(AtomicInt::new(0));
        let handler = Funtimes::new(rcount.clone(), wcount.clone());

        writer.write(&mut buf::wrap("hello".as_bytes())).unwrap();
        event_loop.register(&reader, Token(10)).unwrap();

        let _ = event_loop.run_once(handler);
        let mut b = buf::ByteBuf::new(16);

        assert_eq!((*rcount).load(SeqCst), 1);

        reader.read(&mut b).unwrap();
        b.flip();

        assert_eq!(str::from_utf8(b.bytes()).unwrap(), "hello");
    }
}
