use {Handler, Evented, Poll, NotifyError, Token};
use event::{IoEvent, Interest, PollOpt};
use notify::Notify;
use timer::{Timer, Timeout, TimerResult};
use std::default::Default;
use std::{io, fmt, usize};

/// Configure EventLoop runtime details
#[derive(Copy, Clone, Debug)]
pub struct EventLoopConfig {
    pub io_poll_timeout_ms: usize,

    // == Notifications ==
    pub notify_capacity: usize,
    pub messages_per_tick: usize,

    // == Timer ==
    pub timer_tick_ms: u64,
    pub timer_wheel_size: usize,
    pub timer_capacity: usize,
}

impl Default for EventLoopConfig {
    fn default() -> EventLoopConfig {
        EventLoopConfig {
            io_poll_timeout_ms: 1_000,
            notify_capacity: 4_096,
            messages_per_tick: 256,
            timer_tick_ms: 100,
            timer_wheel_size: 1_024,
            timer_capacity: 65_536,
        }
    }
}

/// Single threaded IO event loop.
pub struct EventLoop<H: Handler> {
    run: bool,
    poll: Poll,
    timer: Timer<H::Timeout>,
    notify: Notify<H::Message>,
    config: EventLoopConfig,
}

// Token used to represent notifications
const NOTIFY: Token = Token(usize::MAX);

impl<H: Handler> EventLoop<H> {

    /// Initializes a new event loop using default configuration settings. The
    /// event loop will not be running yet.
    pub fn new() -> io::Result<EventLoop<H>> {
        EventLoop::configured(Default::default())
    }

    pub fn configured(config: EventLoopConfig) -> io::Result<EventLoop<H>> {
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
        try!(poll.register(&notify, NOTIFY, Interest::readable() | Interest::writable() , PollOpt::edge()));

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
    /// [EventLoopConfig.notify_capacity](struct.EventLoopConfig.html#structfield.notify_capacity).
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
        Sender::new(self.notify.clone())
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
    /// let timeout = event_loop.timeout_ms(123, 300).unwrap();
    /// let _ = event_loop.run(&mut MyHandler);
    /// ```
    pub fn timeout_ms(&mut self, token: H::Timeout, delay: u64) -> TimerResult<Timeout> {
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

    /// Registers an IO handle with the event loop.
    pub fn register<E: Evented>(&mut self, io: &E, token: Token) -> io::Result<()> {
        self.poll.register(io, token, Interest::readable(), PollOpt::level())
    }

    /// Registers an IO handle with the event loop.
    pub fn register_opt<E: Evented>(&mut self, io: &E, token: Token, interest: Interest, opt: PollOpt) -> io::Result<()> {
        self.poll.register(io, token, interest, opt)
    }

    /// Re-Registers an IO handle with the event loop.
    pub fn reregister<E: Evented>(&mut self, io: &E, token: Token, interest: Interest, opt: PollOpt) -> io::Result<()> {
        self.poll.reregister(io, token, interest, opt)
    }

    /// Keep spinning the event loop indefinitely, and notify the handler whenever
    /// any of the registered handles are ready.
    pub fn run(&mut self, handler: &mut H) -> io::Result<()> {
        self.run = true;

        while self.run {
            // Execute ticks as long as the event loop is running
            try!(self.run_once(handler));
        }

        Ok(())
    }

    /// Deregisters an IO handle with the event loop.
    pub fn deregister<E: Evented>(&mut self, io: &E) -> io::Result<()> {
        self.poll.deregister(io)
    }

    /// Spin the event loop once, with a timeout of one second, and notify the
    /// handler if any of the registered handles become ready during that
    /// time.
    pub fn run_once(&mut self, handler: &mut H) -> io::Result<()> {
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
        let events = match self.io_poll(pending) {
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
    fn io_poll(&mut self, immediate: bool) -> io::Result<usize> {
        if immediate {
            self.poll.poll(0)
        } else {
            let mut sleep = self.timer.next_tick_in_ms() as usize;

            if sleep > self.config.io_poll_timeout_ms {
                sleep = self.config.io_poll_timeout_ms;
            }

            self.poll.poll(sleep)
        }
    }

    // Process IO events that have been previously polled
    fn io_process(&mut self, handler: &mut H, cnt: usize) {
        let mut i = 0;

        // Iterate over the notifications. Each event provides the token
        // it was registered with (which usually represents, at least, the
        // handle that the event is about) as well as information about
        // what kind of event occurred (readable, writable, signal, etc.)
        while i < cnt {
            let evt = self.poll.event(i);

            debug!("event={:?}", evt);

            match evt.token() {
                NOTIFY => self.notify.cleanup(),
                _      => self.io_event(handler, evt)
            }

            i += 1;
        }
    }

    fn io_event(&mut self, handler: &mut H, evt: IoEvent) {
        let tok = evt.token();

        if evt.is_readable() | evt.is_error() {
            handler.readable(self, tok, evt.read_hint());
        }

        if evt.is_writable() {
            handler.writable(self, tok);
        }
    }

    fn notify(&mut self, handler: &mut H, mut cnt: usize) {
        while cnt > 0 {
            let msg = self.notify.poll()
                .expect("[BUG] at this point there should always be a message");

            handler.notify(self, msg);
            cnt -= 1;
        }
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

unsafe impl<H: Handler> Sync for EventLoop<H> { }

/// Sends messages to the EventLoop from other threads.
pub struct Sender<M: Send> {
    notify: Notify<M>
}

impl<M: Send> Clone for Sender<M> {
    fn clone(&self) -> Sender<M> {
        Sender { notify: self.notify.clone() }
    }
}

impl<M: Send> fmt::Debug for Sender<M> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Sender<?> {{ ... }}")
    }
}

unsafe impl<M: Send> Sync for Sender<M> { }

impl<M: Send> Sender<M> {
    fn new(notify: Notify<M>) -> Sender<M> {
        Sender { notify: notify }
    }

    pub fn send(&self, msg: M) -> Result<(), NotifyError<M>> {
        self.notify.notify(msg)
    }
}

#[cfg(test)]
mod tests {
    use std::str;
    use std::sync::Arc;
    use std::sync::atomic::AtomicIsize;
    use std::sync::atomic::Ordering::SeqCst;
    use super::EventLoop;
    use {buf, unix, Buf, Handler, Token, TryRead, TryWrite, ReadHint};

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

        fn readable(&mut self, _event_loop: &mut EventLoop<Funtimes>, token: Token, _: ReadHint) {
            (*self.rcount).fetch_add(1, SeqCst);
            assert_eq!(token, Token(10));
        }

        fn writable(&mut self, _event_loop: &mut EventLoop<Funtimes>, token: Token) {
            (*self.wcount).fetch_add(1, SeqCst);
            assert_eq!(token, Token(10));
        }
    }

    #[test]
    pub fn test_readable() {
        let mut event_loop = EventLoop::new().ok().expect("Couldn't make event loop");

        let (mut reader, mut writer) = unix::pipe().unwrap();

        let rcount = Arc::new(AtomicIsize::new(0));
        let wcount = Arc::new(AtomicIsize::new(0));
        let mut handler = Funtimes::new(rcount.clone(), wcount.clone());

        writer.write(&mut buf::SliceBuf::wrap("hello".as_bytes())).unwrap();
        event_loop.register(&reader, Token(10)).unwrap();

        let _ = event_loop.run_once(&mut handler);
        let mut b = buf::ByteBuf::mut_with_capacity(16);

        assert_eq!((*rcount).load(SeqCst), 1);

        reader.read(&mut b).unwrap();

        assert_eq!(str::from_utf8(b.flip().bytes()).unwrap(), "hello");
    }
}
