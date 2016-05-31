use {channel, convert, Handler, Evented, Poll, NotifyError, Token};
use event::{Event, EventSet, PollOpt};
use timer::{OldTimer as Timer, OldTimeout as Timeout, OldTimerResult as TimerResult};
use std::{cmp, io, fmt, usize};
use std::sync::mpsc;
use std::default::Default;
use std::time::Duration;

#[derive(Debug, Default, Clone)]
pub struct EventLoopBuilder {
    config: Config,
}

/// `EventLoop` configuration details
#[derive(Clone, Debug)]
struct Config {
    // == Notifications ==
    notify_capacity: usize,
    messages_per_tick: usize,

    // == Timer ==
    timer_tick: Duration,
    timer_wheel_size: usize,
    timer_capacity: usize,
}

impl Default for Config {
    fn default() -> Config {
        // Default EventLoop configuration values
        Config {
            notify_capacity: 4_096,
            messages_per_tick: 256,
            timer_tick: Duration::from_millis(100),
            timer_wheel_size: 1_024,
            timer_capacity: 65_536,
        }
    }
}

impl EventLoopBuilder {
    /// Construct a new `EventLoopBuilder` with the default configuration
    /// values.
    pub fn new() -> EventLoopBuilder {
        EventLoopBuilder::default()
    }

    /// Sets the maximum number of messages that can be buffered on the event
    /// loop's notification channel before a send will fail.
    ///
    /// The default value for this is 4096.
    pub fn notify_capacity(&mut self, capacity: usize) -> &mut Self {
        self.config.notify_capacity = capacity;
        self
    }

    /// Sets the maximum number of messages that can be processed on any tick of
    /// the event loop.
    ///
    /// The default value for this is 256.
    pub fn messages_per_tick(&mut self, messages: usize) -> &mut Self {
        self.config.messages_per_tick = messages;
        self
    }

    pub fn timer_tick(&mut self, val: Duration) -> &mut Self {
        self.config.timer_tick = val;
        self
    }

    pub fn timer_wheel_size(&mut self, size: usize) -> &mut Self {
        self.config.timer_wheel_size = size;
        self
    }

    pub fn timer_capacity(&mut self, cap: usize) -> &mut Self {
        self.config.timer_capacity = cap;
        self
    }

    /// Constructs a new `EventLoop` using the configured values. The
    /// `EventLoop` will not be running.
    pub fn build<H: Handler>(self) -> io::Result<EventLoop<H>> {
        EventLoop::configured(self.config)
    }
}

/// Single threaded IO event loop.
pub struct EventLoop<H: Handler> {
    run: bool,
    poll: Poll,
    timer: Timer<H::Timeout>,
    notify_tx: channel::Sender<H::Message>,
    notify_rx: channel::Receiver<H::Message>,
    config: Config,
}

// Token used to represent notifications
const NOTIFY: Token = Token(usize::MAX);

impl<H: Handler> EventLoop<H> {

    /// Constructs a new `EventLoop` using the default configuration values.
    /// The `EventLoop` will not be running.
    pub fn new() -> io::Result<EventLoop<H>> {
        EventLoop::configured(Config::default())
    }

    fn configured(config: Config) -> io::Result<EventLoop<H>> {
        // Create the IO poller
        let poll = try!(Poll::new());

        // Create the timer
        let mut timer = Timer::new(
            convert::millis(config.timer_tick),
            config.timer_wheel_size,
            config.timer_capacity);

        // Create cross thread notification queue
        let (tx, rx) = channel::from_std_sync_channel(mpsc::sync_channel(config.notify_capacity));

        // Register the notification wakeup FD with the IO poller
        try!(poll.register(&rx, NOTIFY, EventSet::readable(), PollOpt::edge() | PollOpt::oneshot()));

        // Set the timer's starting time reference point
        timer.setup();

        Ok(EventLoop {
            run: true,
            poll: poll,
            timer: timer,
            notify_tx: tx,
            notify_rx: rx,
            config: config,
        })
    }

    /// Returns a sender that allows sending messages to the event loop in a
    /// thread-safe way, waking up the event loop if needed.
    ///
    /// # Example
    /// ```
    /// use std::thread;
    /// use mio::{EventLoop, Handler};
    ///
    /// struct MyHandler;
    ///
    /// impl Handler for MyHandler {
    ///     type Timeout = ();
    ///     type Message = u32;
    ///
    ///     fn notify(&mut self, event_loop: &mut EventLoop<MyHandler>, msg: u32) {
    ///         assert_eq!(msg, 123);
    ///         event_loop.shutdown();
    ///     }
    /// }
    ///
    /// let mut event_loop = EventLoop::new().unwrap();
    /// let sender = event_loop.channel();
    ///
    /// // Send the notification from another thread
    /// thread::spawn(move || {
    ///     let _ = sender.send(123);
    /// });
    ///
    /// let _ = event_loop.run(&mut MyHandler);
    /// ```
    ///
    /// # Implementation Details
    ///
    /// Each [EventLoop](#) contains a lock-free queue with a pre-allocated
    /// buffer size. The size can be changed by modifying
    /// [EventLoopConfig.notify_capacity](struct.EventLoopConfig.html#method.notify_capacity).
    /// When a message is sent to the EventLoop, it is first pushed on to the
    /// queue. Then, if the EventLoop is currently running, an atomic flag is
    /// set to indicate that the next loop iteration should be started without
    /// waiting.
    ///
    /// If the loop is blocked waiting for IO events, then it is woken up. The
    /// strategy for waking up the event loop is platform dependent. For
    /// example, on a modern Linux OS, eventfd is used. On older OSes, a pipe
    /// is used.
    ///
    /// The strategy of setting an atomic flag if the event loop is not already
    /// sleeping allows avoiding an expensive wakeup operation if at all possible.
    pub fn channel(&self) -> Sender<H::Message> {
        Sender::new(self.notify_tx.clone())
    }

    /// Schedules a timeout after the requested time interval. When the
    /// duration has been reached,
    /// [Handler::timeout](trait.Handler.html#method.timeout) will be invoked
    /// passing in the supplied token.
    ///
    /// Returns a handle to the timeout that can be used to cancel the timeout
    /// using [#clear_timeout](#method.clear_timeout).
    ///
    /// # Example
    /// ```
    /// use mio::{EventLoop, Handler};
    /// use std::time::Duration;
    ///
    /// struct MyHandler;
    ///
    /// impl Handler for MyHandler {
    ///     type Timeout = u32;
    ///     type Message = ();
    ///
    ///     fn timeout(&mut self, event_loop: &mut EventLoop<MyHandler>, timeout: u32) {
    ///         assert_eq!(timeout, 123);
    ///         event_loop.shutdown();
    ///     }
    /// }
    ///
    ///
    /// let mut event_loop = EventLoop::new().unwrap();
    /// let timeout = event_loop.timeout(123, Duration::from_millis(300)).unwrap();
    /// let _ = event_loop.run(&mut MyHandler);
    /// ```
    pub fn timeout(&mut self, token: H::Timeout, delay: Duration) -> TimerResult<Timeout> {
        self.timer.timeout_ms(token, convert::millis(delay))
    }

    /// If the supplied timeout has not been triggered, cancel it such that it
    /// will not be triggered in the future.
    pub fn clear_timeout(&mut self, timeout: &Timeout) -> bool {
        self.timer.clear(&timeout)
    }

    /// Tells the event loop to exit after it is done handling all events in the
    /// current iteration.
    pub fn shutdown(&mut self) {
        self.run = false;
    }

    /// Indicates whether the event loop is currently running. If it's not it has either
    /// stopped or is scheduled to stop on the next tick.
    pub fn is_running(&self) -> bool {
        self.run
    }

    /// Registers an IO handle with the event loop.
    pub fn register<E: ?Sized>(&mut self, io: &E, token: Token, interest: EventSet, opt: PollOpt) -> io::Result<()>
        where E: Evented
    {
        self.poll.register(io, token, interest, opt)
    }

    /// Re-Registers an IO handle with the event loop.
    pub fn reregister<E: ?Sized>(&mut self, io: &E, token: Token, interest: EventSet, opt: PollOpt) -> io::Result<()>
        where E: Evented
    {
        self.poll.reregister(io, token, interest, opt)
    }

    /// Keep spinning the event loop indefinitely, and notify the handler whenever
    /// any of the registered handles are ready.
    pub fn run(&mut self, handler: &mut H) -> io::Result<()> {
        self.run = true;

        while self.run {
            // Execute ticks as long as the event loop is running
            try!(self.run_once(handler, None));
        }

        Ok(())
    }

    /// Deregisters an IO handle with the event loop.
    pub fn deregister<E: ?Sized>(&mut self, io: &E) -> io::Result<()> where E: Evented {
        self.poll.deregister(io)
    }

    /// Spin the event loop once, with a timeout of one second, and notify the
    /// handler if any of the registered handles become ready during that
    /// time.
    pub fn run_once(&mut self, handler: &mut H, timeout: Option<Duration>) -> io::Result<()> {
        trace!("event loop tick");

        // Check the registered IO handles for any new events. Each poll
        // is for one second, so a shutdown request can last as long as
        // one second before it takes effect.
        let events = match self.io_poll(timeout) {
            Ok(e) => e,
            Err(err) => {
                if err.kind() == io::ErrorKind::Interrupted {
                    handler.interrupted(self);
                    0
                } else {
                    return Err(err);
                }
            }
        };

        self.io_process(handler, events);
        self.timer_process(handler);
        handler.tick(self);
        Ok(())
    }

    #[inline]
    fn io_poll(&mut self, timeout: Option<Duration>) -> io::Result<usize> {
        let next_tick = self.timer.next_tick_in_ms()
            .map(|ms| cmp::min(ms, usize::MAX as u64));

        let timeout = match (timeout, next_tick) {
            (Some(a), Some(b)) => Some(cmp::min(a, Duration::from_millis(b))),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(Duration::from_millis(b)),
            _ => None,
        };

        self.poll.poll(timeout)
    }

    // Process IO events that have been previously polled
    fn io_process(&mut self, handler: &mut H, cnt: usize) {
        let mut i = 0;

        trace!("io_process(..); cnt={}; len={}", cnt, self.poll.events().len());

        // Iterate over the notifications. Each event provides the token
        // it was registered with (which usually represents, at least, the
        // handle that the event is about) as well as information about
        // what kind of event occurred (readable, writable, signal, etc.)
        while i < cnt {
            let evt = self.poll.events().get(i).unwrap();

            trace!("event={:?}; idx={:?}", evt, i);

            match evt.token() {
                NOTIFY => self.notify(handler),
                _ => self.io_event(handler, evt)
            }

            i += 1;
        }
    }

    fn io_event(&mut self, handler: &mut H, evt: Event) {
        handler.ready(self, evt.token(), evt.kind());
    }

    fn notify(&mut self, handler: &mut H) {
        for _ in 0..self.config.messages_per_tick {
            match self.notify_rx.try_recv() {
                Ok(msg) => handler.notify(self, msg),
                _ => break,
            }
        }

        // Re-register
        let _ = self.poll.reregister(&self.notify_rx, NOTIFY, EventSet::readable(), PollOpt::edge() | PollOpt::oneshot());
    }

    fn timer_process(&mut self, handler: &mut H) {
        let now = self.timer.now();

        loop {
            match self.timer.tick_to(now) {
                Some(t) => handler.timeout(self, t),
                _ => return
            }
        }
    }
}

impl<H: Handler> fmt::Debug for EventLoop<H> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("EventLoop")
            .field("run", &self.run)
            .field("poll", &self.poll)
            .field("config", &self.config)
            .finish()
    }
}

/// Sends messages to the EventLoop from other threads.
pub struct Sender<M> {
    tx: channel::Sender<M>
}

impl<M> fmt::Debug for Sender<M> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Sender<?> {{ ... }}")
    }
}

impl<M> Sender<M> {
    fn new(tx: channel::Sender<M>) -> Sender<M> {
        Sender { tx: tx }
    }

    pub fn send(&self, msg: M) -> Result<(), NotifyError<M>> {
        try!(self.tx.try_send(msg));
        Ok(())
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use {unix, EventLoop, Handler, Token, TryRead, TryWrite, EventSet, PollOpt};
    use bytes::{Buf, SliceBuf, ByteBuf};
    use std::str;
    use std::sync::Arc;
    use std::sync::atomic::AtomicIsize;
    use std::sync::atomic::Ordering::SeqCst;
    use std::time::Duration;

    #[test]
    pub fn test_event_loop_size() {
        use std::mem;
        assert!(512 >= mem::size_of::<EventLoop<Funtimes>>());
    }

    struct Funtimes {
        rcount: Arc<AtomicIsize>,
        wcount: Arc<AtomicIsize>
    }

    impl Funtimes {
        fn new(rcount: Arc<AtomicIsize>, wcount: Arc<AtomicIsize>) -> Funtimes {
            Funtimes {
                rcount: rcount,
                wcount: wcount
            }
        }
    }

    impl Handler for Funtimes {
        type Timeout = usize;
        type Message = ();

        fn ready(&mut self, _event_loop: &mut EventLoop<Funtimes>, token: Token, events: EventSet) {
            if events.is_readable() {
                (*self.rcount).fetch_add(1, SeqCst);
                assert_eq!(token, Token(10));
            }

            if events.is_writable() {
                (*self.wcount).fetch_add(1, SeqCst);
                assert_eq!(token, Token(10));
            }
        }
    }

    #[test]
    pub fn test_readable() {
        let mut event_loop = EventLoop::new().ok().expect("Couldn't make event loop");

        let (mut reader, mut writer) = unix::pipe().unwrap();

        let rcount = Arc::new(AtomicIsize::new(0));
        let wcount = Arc::new(AtomicIsize::new(0));
        let mut handler = Funtimes::new(rcount.clone(), wcount.clone());

        writer.try_write_buf(&mut SliceBuf::wrap(b"hello")).unwrap();
        event_loop.register(&reader, Token(10), EventSet::readable(),
                            PollOpt::edge()).unwrap();

        let _ = event_loop.run_once(&mut handler, None);
        let mut b = ByteBuf::mut_with_capacity(16);

        assert_eq!((*rcount).load(SeqCst), 1);

        reader.try_read_buf(&mut b).unwrap();

        assert_eq!(str::from_utf8(b.flip().bytes()).unwrap(), "hello");
    }

    pub struct BrokenPipeHandler;

    impl Handler for BrokenPipeHandler {
        type Timeout = ();
        type Message = ();
        fn ready(&mut self, _: &mut EventLoop<Self>, token: Token, _: EventSet) {
            if token == Token(1) {
                panic!("Received ready() on a closed pipe.");
            }
        }
    }

    #[test]
    pub fn broken_pipe() {
        let mut event_loop: EventLoop<BrokenPipeHandler> = EventLoop::new().unwrap();
        let (reader, _) = unix::pipe().unwrap();

        // On Darwin this returns a "broken pipe" error.
        let _ = event_loop.register(&reader, Token(1), EventSet::all(), PollOpt::edge());

        let mut handler = BrokenPipeHandler;
        drop(reader);
        event_loop.run_once(&mut handler, Some(Duration::from_millis(1000))).unwrap();
    }
}
