use token::Token;
use std::time::{Instant, Duration};
use std::{error, fmt, usize, iter};

use self::TimerErrorKind::TimerOverflow;
const EMPTY: Token = Token(usize::MAX);
const NS_PER_S: u64 = 1_000_000_000;

#[derive(Debug, Clone, Copy)]
pub struct Tick (u64);

// Implements coarse-grained timeouts using an algorithm based on hashed timing
// wheels by Varghese & Lauck.
//
// TODO:
// * Handle the case when the timer falls more than an entire wheel behind. There
//   is no point to loop multiple times around the wheel in one go.
// * New type for tick, now() -> Tick
#[derive(Debug)]
pub struct Timer<T> {
    // Size of each tick
    tick_size: Duration,
    // Slab of timeout entries
    entries: Slab<Entry<T>>,
    // Timeout wheel. Each tick, the timer will look at the next slot for
    // timeouts that match the current tick.
    wheel: Vec<Token>,
    // Tick 0's time in nan
    start: Instant,
    // The current tick
    tick: Tick,
    // The next entry to possibly timeout
    next: Token,
    // Masks the target tick to get the slot
    mask: u64,
}

#[derive(Clone)]
pub struct Timeout {
    // Reference into the timer entry slab
    token: Token,
    // Tick that it should matchup with
    tick: Tick,
}

type Slab<T> = ::slab::Slab<T, ::Token>;

impl<T> Timer<T> {
    pub fn new(tick_sz: Duration, mut slots: usize, mut capacity: usize) -> Timer<T> {
        slots = slots.next_power_of_two();
        capacity = capacity.next_power_of_two();

        Timer {
            tick_size: tick_sz,
            entries: Slab::new(capacity),
            wheel: iter::repeat(EMPTY).take(slots).collect(),
            start: Instant::now(),
            tick: Tick(0),
            next: EMPTY,
            mask: (slots as u64) - 1
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.entries.count()
    }

    // Time remaining until the next tick
    pub fn next_tick(&self) -> Option<Duration> {

        if self.entries.count() == 0 {
            return None;
        }

        let now = Instant::now();
        let nxt = self.start + self.tick_to_duration(Tick(self.tick.0 + 1));

        if nxt <= now {
            return Some(Duration::new(0,0));
        }

        Some(nxt - now)
    }

    /*
     *
     * ===== Initialization =====
     *
     */

    // Sets the starting time of the timer using the current system time
    pub fn setup(&mut self) {
        self.start = Instant::now();
    }

    /*
     *
     * ===== Timeout create / cancel =====
     *
     */

    pub fn timeout(&mut self, token: T, delay: Duration) -> TimerResult<Timeout> {
        let at = Instant::now() + delay;
        self.timeout_at(token, at)
    }

    pub fn timeout_at(&mut self, token: T, at: Instant) -> TimerResult<Timeout> {

        // Make relative to start -- we pad it to the next tick up
        let span = (at - self.start) + (self.tick_size - Duration::from_millis(1));

        // Calculate tick
        let mut tick = self.duration_to_tick(span);

        // Always target at least 1 tick in the future
        if tick.0 <= self.tick.0 {
            tick = Tick(self.tick.0 + 1);
        }

        self.insert(token, tick)
    }

    pub fn clear(&mut self, timeout: &Timeout) -> bool {
        let links = match self.entries.get(timeout.token) {
            Some(e) => e.links,
            None => return false
        };

        // Sanity check
        if links.tick != timeout.tick.0 {
            return false;
        }

        self.unlink(&links, timeout.token);
        self.entries.remove(timeout.token);
        true
    }

    fn insert(&mut self, token: T, tick: Tick) -> TimerResult<Timeout> {
        // Get the slot for the requested tick
        let slot = self.slot_for(tick);
        let curr = self.wheel[slot];

        // Insert the new entry
        let token = try!(
            self.entries.insert(Entry::new(token, tick.0, curr))
            .map_err(|_| TimerError::overflow()));

        if curr != EMPTY {
            // If there was a previous entry, set its prev pointer to the new
            // entry
            self.entries[curr].links.prev = token;
        }

        // Update the head slot
        self.wheel[slot] = token;

        trace!("inserted timout; slot={}; token={:?}", slot, token);

        // Return the new timeout
        Ok(Timeout {
            token: token,
            tick: tick
        })
    }

    fn unlink(&mut self, links: &EntryLinks, token: Token) {
       trace!("unlinking timeout; slot={}; token={:?}",
               self.slot_for(Tick(links.tick)), token);

        if links.prev == EMPTY {
            let slot = self.slot_for(Tick(links.tick));
            self.wheel[slot] = links.next;
        } else {
            self.entries[links.prev].links.next = links.next;
        }

        if links.next != EMPTY {
            self.entries[links.next].links.prev = links.prev;

            if token == self.next {
                self.next = links.next;
            }
        } else if token == self.next {
            self.next = EMPTY;
        }
    }

    /*
     *
     * ===== Advance time =====
     *
     */

    pub fn now(&self) -> Tick {
        self.duration_to_tick(Instant::now() - self.start)
    }

    pub fn tick_to(&mut self, now: Tick) -> Option<T> {
        trace!("tick_to; now={}; tick={}", now, self.tick);

        while self.tick.0 <= now.0 {
            let curr = self.next;

            trace!("ticking; curr={:?}", curr);

            if curr == EMPTY {
                self.tick = Tick(self.tick.0 + 1);
                self.next = self.wheel[self.slot_for(self.tick)];
            } else {
                let links = self.entries[curr].links;

                if links.tick <= self.tick.0 {
                    trace!("triggering; token={:?}", curr);

                    // Unlink will also advance self.next
                    self.unlink(&links, curr);

                    // Remove and return the token
                    return self.entries.remove(curr)
                        .map(|e| e.token);
                } else {
                    self.next = links.next;
                }
            }
        }

        None
    }

    /*
     *
     * ===== Misc =====
     *
     */

    #[inline]
    fn slot_for(&self, tick: Tick) -> usize {
        (self.mask & tick.0) as usize
    }

    // Convert a duration into a number of ticks
    // to make sense, the duration probably needs to be the span
    // of time relative to the start of the Timer object
    #[inline]
    fn duration_to_tick(&self, t: Duration) -> Tick {
        let ns = (t.as_secs() * NS_PER_S) + t.subsec_nanos() as u64;
        let sz = (self.tick_size.as_secs() * NS_PER_S) + self.tick_size.subsec_nanos() as u64;
        Tick(ns / sz)
    }

    // Converts a number of ticks to a time span using a supplied size
    #[inline]
    fn tick_to_duration(&self, t: Tick) -> Duration {
        let sz = (self.tick_size.as_secs() * NS_PER_S) + self.tick_size.subsec_nanos() as u64;
        let t = sz * t.0;
        Duration::new(0, t as u32)
    }
}

// Doubly linked list of timer entries. Allows for efficient insertion /
// removal of timeouts.
struct Entry<T> {
    token: T,
    links: EntryLinks,
}

impl<T> Entry<T> {
    fn new(token: T, tick: u64, next: Token) -> Entry<T> {
        Entry {
            token: token,
            links: EntryLinks {
                tick: tick,
                prev: EMPTY,
                next: next,
            },
        }
    }
}

#[derive(Copy, Clone)]
struct EntryLinks {
    tick: u64,
    prev: Token,
    next: Token
}

pub type TimerResult<T> = Result<T, TimerError>;

#[derive(Debug)]
pub struct TimerError {
    kind: TimerErrorKind,
    desc: &'static str,
}

impl fmt::Display for TimerError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}: {}", self.kind, self.desc)
    }
}

impl TimerError {
    fn overflow() -> TimerError {
        TimerError {
            kind: TimerOverflow,
            desc: "too many timer entries"
        }
    }
}

impl error::Error for TimerError {
    fn description(&self) -> &str {
        self.desc
    }
}

#[derive(Debug)]
pub enum TimerErrorKind {
    TimerOverflow,
}

impl fmt::Display for TimerErrorKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            TimerOverflow => write!(fmt, "TimerOverflow"),
        }
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Tick({})", self.0)
    }
}

#[cfg(test)]
mod test {
    use super::Timer;
    use std::time::Duration;

    #[test]
    pub fn test_timeout_next_tick() {
        let mut t = timer();
        let mut tick;

        t.timeout("a", Duration::from_millis(100)).unwrap();

        tick = t.duration_to_tick(Duration::from_millis(50));
        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(100));
        assert_eq!(Some("a"), t.tick_to(tick));
        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(150));
        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(200));
        assert_eq!(None, t.tick_to(tick));

        assert_eq!(t.count(), 0);
    }

    #[test]
    pub fn test_clearing_timeout() {
        let mut t = timer();
        let mut tick;

        let to = t.timeout("a", Duration::from_millis(100)).unwrap();
        assert!(t.clear(&to));

        tick = t.duration_to_tick(Duration::from_millis(100));
        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(200));
        assert_eq!(None, t.tick_to(tick));

        assert_eq!(t.count(), 0);
    }

    #[test]
    pub fn test_multiple_timeouts_same_tick() {
        let mut t = timer();
        let mut tick;

        t.timeout("a", Duration::from_millis(100)).unwrap();
        t.timeout("b", Duration::from_millis(100)).unwrap();

        let mut rcv = vec![];

        tick = t.duration_to_tick(Duration::from_millis(100));
        rcv.push(t.tick_to(tick).unwrap());
        rcv.push(t.tick_to(tick).unwrap());

        assert_eq!(None, t.tick_to(tick));

        rcv.sort();
        assert!(rcv == ["a", "b"], "actual={:?}", rcv);

        tick = t.duration_to_tick(Duration::from_millis(200));
        assert_eq!(None, t.tick_to(tick));

        assert_eq!(t.count(), 0);
    }

    #[test]
    pub fn test_multiple_timeouts_diff_tick() {
        let mut t = timer();
        let mut tick;

        t.timeout("a", Duration::from_millis(110)).unwrap();
        t.timeout("b", Duration::from_millis(220)).unwrap();
        t.timeout("c", Duration::from_millis(230)).unwrap();
        t.timeout("d", Duration::from_millis(440)).unwrap();

        tick = t.duration_to_tick(Duration::from_millis(100));
        trace!("{}", tick.0);
        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(200));
        assert_eq!(Some("a"), t.tick_to(tick));
        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(300));
        assert_eq!(Some("c"), t.tick_to(tick));
        assert_eq!(Some("b"), t.tick_to(tick));
        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(400));
        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(500));
        assert_eq!(Some("d"), t.tick_to(tick));
        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(600));
        assert_eq!(None, t.tick_to(tick));
    }

    #[test]
    pub fn test_catching_up() {
        let mut t = timer();

        t.timeout("a", Duration::from_millis(110)).unwrap();
        t.timeout("b", Duration::from_millis(220)).unwrap();
        t.timeout("c", Duration::from_millis(230)).unwrap();
        t.timeout("d", Duration::from_millis(440)).unwrap();

        let tick = t.duration_to_tick(Duration::from_millis(600));
        assert_eq!(Some("a"), t.tick_to(tick));
        assert_eq!(Some("c"), t.tick_to(tick));
        assert_eq!(Some("b"), t.tick_to(tick));
        assert_eq!(Some("d"), t.tick_to(tick));
        assert_eq!(None, t.tick_to(tick));
    }

    #[test]
    pub fn test_timeout_hash_collision() {
        let mut t = timer();
        let mut tick;

        t.timeout("a", Duration::from_millis(100)).unwrap();
        t.timeout("b", Duration::from_millis(100 + TICK * SLOTS as u64)).unwrap();

        tick = t.duration_to_tick(Duration::from_millis(100));
        assert_eq!(Some("a"), t.tick_to(tick));
        assert_eq!(1, t.count());

        tick = t.duration_to_tick(Duration::from_millis(200));
        assert_eq!(None, t.tick_to(tick));
        assert_eq!(1, t.count());

        tick = t.duration_to_tick(Duration::from_millis(100 + TICK * SLOTS as u64));
        assert_eq!(Some("b"), t.tick_to(tick));
        assert_eq!(0, t.count());
    }

    #[test]
    pub fn test_clearing_timeout_between_triggers() {
        let mut t = timer();
        let mut tick;

        let a = t.timeout("a", Duration::from_millis(100)).unwrap();
        let _ = t.timeout("b", Duration::from_millis(100)).unwrap();
        let _ = t.timeout("c", Duration::from_millis(200)).unwrap();

        tick = t.duration_to_tick(Duration::from_millis(100));
        assert_eq!(Some("b"), t.tick_to(tick));
        assert_eq!(2, t.count());

        t.clear(&a);
        assert_eq!(1, t.count());

        assert_eq!(None, t.tick_to(tick));

        tick = t.duration_to_tick(Duration::from_millis(200));
        assert_eq!(Some("c"), t.tick_to(tick));
        assert_eq!(0, t.count());
    }

    const TICK: u64 = 100;
    const SLOTS: usize = 16;

    fn timer() -> Timer<&'static str> {
        Timer::new(Duration::from_millis(TICK), SLOTS, 32)
    }
}
