use std::default::Default;
use std::time::duration::Duration;
use std::{fmt, usize};
use error::{MioResult, MioError};
use handler::Handler;
use io::IoHandle;
use notify::Notify;
use os::event;
use poll::{Poll};
use timer::{Timer, Timeout, TimerResult};
use os::token::Token;

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
            notify_capacity: 1_024,
            messages_per_tick: 64,
            timer_tick_ms: 100,
            timer_wheel_size: 1_024,
            timer_capacity: 65_536,
        }
    }
}

/// Single threaded IO event loop.
#[derive(Debug)]
pub struct EventLoop<T, M: Send> {
    run: bool,
    poll: Poll,
    timer: Timer<T>,
    notify: Notify<M>,
    config: EventLoopConfig,
}

// Token used to represent notifications
const NOTIFY: Token = Token(usize::MAX);

impl<T, M: Send> EventLoop<T, M> {

    /// Initializes a new event loop using default configuration settings. The
    /// event loop will not be running yet.
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
        try!(poll.register(&notify, NOTIFY, event::READABLE | event::WRITABLE, event::EDGE));

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
    /// #![allow(unstable)]
    ///
    /// use std::thread::Thread;
    /// use mio::{EventLoop, Handler};
    ///
    /// struct MyHandler;
    ///
    /// impl Handler<(), u32> for MyHandler {
    ///     fn notify(&mut self, event_loop: &mut EventLoop<(), u32>, msg: u32) {
    ///         assert_eq!(msg, 123);
    ///         event_loop.shutdown();
    ///     }
    /// }
    ///
    /// let mut event_loop = EventLoop::new().unwrap();
    /// let sender = event_loop.channel();
    ///
    /// // Send the notification from another thread
    /// Thread::spawn(move || {
    ///     let _ = sender.send(123);
    /// });
    ///
    /// let _ = event_loop.run(MyHandler);
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
    pub fn channel(&self) -> EventLoopSender<M> {
        EventLoopSender::new(self.notify.clone())
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
    /// #![allow(unstable)]
    ///
    /// use mio::{EventLoop, Handler};
    /// use std::time::Duration;
    ///
    /// struct MyHandler;
    ///
    /// impl Handler<u32, ()> for MyHandler {
    ///     fn timeout(&mut self, event_loop: &mut EventLoop<u32, ()>, timeout: u32) {
    ///         assert_eq!(timeout, 123);
    ///         event_loop.shutdown();
    ///     }
    /// }
    ///
    ///
    /// let mut event_loop = EventLoop::new().unwrap();
    /// let timeout = event_loop.timeout(123, Duration::milliseconds(300)).unwrap();
    /// let _ = event_loop.run(MyHandler);
    /// ```
    pub fn timeout(&mut self, token: T, delay: Duration) -> TimerResult<Timeout> {
        self.timer.timeout(token, delay)
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
    pub fn register<H: IoHandle>(&mut self, io: &H, token: Token) -> MioResult<()> {
        self.poll.register(io, token, event::READABLE, event::LEVEL)
    }

    /// Registers an IO handle with the event loop.
    pub fn register_opt<H: IoHandle>(&mut self, io: &H, token: Token, interest: event::Interest, opt: event::PollOpt) -> MioResult<()> {
        self.poll.register(io, token, interest, opt)
    }

    /// Re-Registers an IO handle with the event loop.
    pub fn reregister<H: IoHandle>(&mut self, io: &H, token: Token, interest: event::Interest, opt: event::PollOpt) -> MioResult<()> {
        self.poll.reregister(io, token, interest, opt)
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

    /// Deregisters an IO handle with the event loop.
    pub fn deregister<H: IoHandle>(&mut self, io: &H) -> MioResult<()> {
        self.poll.deregister(io)
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
    fn io_poll(&mut self, immediate: bool) -> MioResult<usize> {
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
    fn io_process<H: Handler<T, M>>(&mut self, handler: &mut H, cnt: usize) {
        let mut i = 0us;

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

    fn io_event<H: Handler<T, M>>(&mut self, handler: &mut H, evt: event::IoEvent) {
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

    fn notify<H: Handler<T, M>>(&mut self, handler: &mut H, mut cnt: usize) {
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

unsafe impl<T, M: Send> Sync for EventLoop<T, M> { }

/// Sends messages to the EventLoop from other threads.
pub struct EventLoopSender<M: Send> {
    notify: Notify<M>
}

impl<M: Send> Clone for EventLoopSender<M> {
    fn clone(&self) -> EventLoopSender<M> {
        EventLoopSender { notify: self.notify.clone() }
    }
}

impl<M: Send> fmt::Debug for EventLoopSender<M> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "EventLoopSender<?> {{ ... }}")
    }
}

unsafe impl<M: Send> Sync for EventLoopSender<M> { }

impl<M: Send> EventLoopSender<M> {
    fn new(notify: Notify<M>) -> EventLoopSender<M> {
        EventLoopSender { notify: notify }
    }

    pub fn send(&self, msg: M) -> Result<(), M> {
        self.notify.notify(msg)
    }
}

pub type EventLoopResult<H> = Result<H, EventLoopError<H>>;

#[derive(Debug)]
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
    use std::sync::atomic::AtomicIsize;
    use std::sync::atomic::Ordering::SeqCst;
    use super::EventLoop;
    use io::{IoWriter, IoReader};
    use {io, buf, Buf, Handler, Token};
    use os::event;

    type TestEventLoop = EventLoop<usize, ()>;

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

    impl Handler<usize, ()> for Funtimes {
        fn readable(&mut self, _event_loop: &mut TestEventLoop, token: Token, _hint: event::ReadHint) {
            (*self.rcount).fetch_add(1, SeqCst);
            assert_eq!(token, Token(10));
        }
    }

    #[test]
    fn test_readable() {
        let mut event_loop = EventLoop::new().ok().expect("Couldn't make event loop");

        let (reader, writer) = io::pipe().unwrap();

        let rcount = Arc::new(AtomicIsize::new(0));
        let wcount = Arc::new(AtomicIsize::new(0));
        let handler = Funtimes::new(rcount.clone(), wcount.clone());

        writer.write(&mut buf::SliceBuf::wrap("hello".as_bytes())).unwrap();
        event_loop.register(&reader, Token(10)).unwrap();

        let _ = event_loop.run_once(handler);
        let mut b = buf::ByteBuf::mut_with_capacity(16);

        assert_eq!((*rcount).load(SeqCst), 1);

        reader.read(&mut b).unwrap();

        assert_eq!(str::from_utf8(b.flip().bytes()).unwrap(), "hello");
    }
}
