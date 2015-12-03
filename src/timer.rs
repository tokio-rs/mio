use token::Token;
use util::Slab;
use time::precise_time_ns;
use std::{usize, iter};
use std::cmp::max;

use self::TimerErrorKind::TimerOverflow;

const EMPTY: Token = Token(usize::MAX);
const NS_PER_MS: u64 = 1_000_000;

// Implements coarse-grained timeouts using an algorithm based on hashed timing
// wheels by Varghese & Lauck.
//
// TODO:
// * Handle the case when the timer falls more than an entire wheel behind. There
//   is no point to loop multiple times around the wheel in one go.
// * New type for tick, now() -> Tick
#[derive(Debug)]
pub struct Timer<T> {
    // Size of each tick in milliseconds
    tick_ms: u64,
    // Slab of timeout entries
    entries: Slab<Entry<T>>,
    // Timeout wheel. Each tick, the timer will look at the next slot for
    // timeouts that match the current tick.
    wheel: Vec<Token>,
    // Tick 0's time in milliseconds
    start: u64,
    // The current tick
    tick: u64,
    // The next entry to possibly timeout
    next: Token,
    // Masks the target tick to get the slot
    mask: u64,
}

#[derive(Copy, Clone)]
pub struct Timeout {
    // Reference into the timer entry slab
    token: Token,
    // Tick that it should matchup with
    tick: u64,
}

impl<T> Timer<T> {
    pub fn new(tick_ms: u64, mut slots: usize, mut capacity: usize) -> Timer<T> {
        slots = slots.next_power_of_two();
        capacity = capacity.next_power_of_two();

        Timer {
            tick_ms: tick_ms,
            entries: Slab::new(capacity),
            wheel: iter::repeat(EMPTY).take(slots).collect(),
            start: 0,
            tick: 0,
            next: EMPTY,
            mask: (slots as u64) - 1
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.entries.count()
    }

    // Number of ms remaining until the next tick
    pub fn next_tick_in_ms(&self) -> Option<u64> {
        if self.entries.count() == 0 {
            return None;
        }

        let now = self.now_ms();
        let nxt = self.start + (self.tick + 1) * self.tick_ms;

        if nxt <= now {
            return Some(0);
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
        let now = self.now_ms();
        self.set_start_ms(now);
    }

    fn set_start_ms(&mut self, start: u64) {
        assert!(!self.is_initialized(), "the timer has already started");
        self.start = start;
    }

    /*
     *
     * ===== Timeout create / cancel =====
     *
     */

    pub fn timeout_ms(&mut self, token: T, delay: u64) -> TimerResult<Timeout> {
        let at = self.now_ms() + max(0, delay);
        self.timeout_at_ms(token, at)
    }

    pub fn timeout_at_ms(&mut self, token: T, mut at: u64) -> TimerResult<Timeout> {
        // Make relative to start
        at -= self.start;
        // Calculate tick
        let mut tick = (at + self.tick_ms - 1) / self.tick_ms;

        // Always target at least 1 tick in the future
        if tick <= self.tick {
            tick = self.tick + 1;
        }

        self.insert(token, tick)
    }

    pub fn clear(&mut self, timeout: Timeout) -> bool {
        let links = match self.entries.get(timeout.token) {
            Some(e) => e.links,
            None => return false
        };

        // Sanity check
        if links.tick != timeout.tick {
            return false;
        }

        self.unlink(&links, timeout.token);
        self.entries.remove(timeout.token);
        true
    }

    fn insert(&mut self, token: T, tick: u64) -> TimerResult<Timeout> {
        // Get the slot for the requested tick
        let slot = (tick & self.mask) as usize;
        let curr = self.wheel[slot];

        // Insert the new entry
        let token = try!(
            self.entries.insert(Entry::new(token, tick, curr))
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
               self.slot_for(links.tick), token);

        if links.prev == EMPTY {
            let slot = self.slot_for(links.tick);
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

    pub fn now(&self) -> u64 {
        self.ms_to_tick(self.now_ms())
    }

    pub fn tick_to(&mut self, now: u64) -> Option<T> {
        trace!("tick_to; now={}; tick={}", now, self.tick);

        while self.tick <= now {
            let curr = self.next;

            trace!("ticking; curr={:?}", curr);

            if curr == EMPTY {
                self.tick += 1;
                self.next = self.wheel[self.slot_for(self.tick)];
            } else {
                let links = self.entries[curr].links;

                if links.tick <= self.tick {
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

    // Timers are initialized when either the current time has been advanced or a timeout has been set
    #[inline]
    fn is_initialized(&self) -> bool {
        self.tick > 0 || !self.entries.is_empty()
    }

    #[inline]
    fn slot_for(&self, tick: u64) -> usize {
        (self.mask & tick) as usize
    }

    // Convert a ms duration into a number of ticks, rounds up
    #[inline]
    fn ms_to_tick(&self, ms: u64) -> u64 {
        (ms - self.start) / self.tick_ms
    }

    #[inline]
    fn now_ms(&self) -> u64 {
        precise_time_ns() / NS_PER_MS
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

impl TimerError {
    fn overflow() -> TimerError {
        TimerError {
            kind: TimerOverflow,
            desc: "too many timer entries"
        }
    }
}

#[derive(Debug)]
pub enum TimerErrorKind {
    TimerOverflow,
}

#[cfg(test)]
mod test {
    use super::Timer;

    #[test]
    pub fn test_timeout_next_tick() {
        let mut t = timer();
        let mut tick;

        t.timeout_at_ms("a", 100).unwrap();

        tick = t.ms_to_tick(50);
        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(100);
        assert_eq!(Some("a"), t.tick_to(tick));
        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(150);
        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(200);
        assert_eq!(None, t.tick_to(tick));

        assert_eq!(t.count(), 0);
    }

    #[test]
    pub fn test_clearing_timeout() {
        let mut t = timer();
        let mut tick;

        let to = t.timeout_at_ms("a", 100).unwrap();
        assert!(t.clear(to));

        tick = t.ms_to_tick(100);
        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(200);
        assert_eq!(None, t.tick_to(tick));

        assert_eq!(t.count(), 0);
    }

    #[test]
    pub fn test_multiple_timeouts_same_tick() {
        let mut t = timer();
        let mut tick;

        t.timeout_at_ms("a", 100).unwrap();
        t.timeout_at_ms("b", 100).unwrap();

        let mut rcv = vec![];

        tick = t.ms_to_tick(100);
        rcv.push(t.tick_to(tick).unwrap());
        rcv.push(t.tick_to(tick).unwrap());

        assert_eq!(None, t.tick_to(tick));

        rcv.sort();
        assert!(rcv == ["a", "b"], "actual={:?}", rcv);

        tick = t.ms_to_tick(200);
        assert_eq!(None, t.tick_to(tick));

        assert_eq!(t.count(), 0);
    }

    #[test]
    pub fn test_multiple_timeouts_diff_tick() {
        let mut t = timer();
        let mut tick;

        t.timeout_at_ms("a", 110).unwrap();
        t.timeout_at_ms("b", 220).unwrap();
        t.timeout_at_ms("c", 230).unwrap();
        t.timeout_at_ms("d", 440).unwrap();

        tick = t.ms_to_tick(100);
        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(200);
        assert_eq!(Some("a"), t.tick_to(tick));
        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(300);
        assert_eq!(Some("c"), t.tick_to(tick));
        assert_eq!(Some("b"), t.tick_to(tick));
        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(400);
        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(500);
        assert_eq!(Some("d"), t.tick_to(tick));
        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(600);
        assert_eq!(None, t.tick_to(tick));
    }

    #[test]
    pub fn test_catching_up() {
        let mut t = timer();

        t.timeout_at_ms("a", 110).unwrap();
        t.timeout_at_ms("b", 220).unwrap();
        t.timeout_at_ms("c", 230).unwrap();
        t.timeout_at_ms("d", 440).unwrap();

        let tick = t.ms_to_tick(600);
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

        t.timeout_at_ms("a", 100).unwrap();
        t.timeout_at_ms("b", 100 + TICK * SLOTS as u64).unwrap();

        tick = t.ms_to_tick(100);
        assert_eq!(Some("a"), t.tick_to(tick));
        assert_eq!(1, t.count());

        tick = t.ms_to_tick(200);
        assert_eq!(None, t.tick_to(tick));
        assert_eq!(1, t.count());

        tick = t.ms_to_tick(100 + TICK * SLOTS as u64);
        assert_eq!(Some("b"), t.tick_to(tick));
        assert_eq!(0, t.count());
    }

    #[test]
    pub fn test_clearing_timeout_between_triggers() {
        let mut t = timer();
        let mut tick;

        let a = t.timeout_at_ms("a", 100).unwrap();
        let _ = t.timeout_at_ms("b", 100).unwrap();
        let _ = t.timeout_at_ms("c", 200).unwrap();

        tick = t.ms_to_tick(100);
        assert_eq!(Some("b"), t.tick_to(tick));
        assert_eq!(2, t.count());

        t.clear(a);
        assert_eq!(1, t.count());

        assert_eq!(None, t.tick_to(tick));

        tick = t.ms_to_tick(200);
        assert_eq!(Some("c"), t.tick_to(tick));
        assert_eq!(0, t.count());
    }

    const TICK: u64 = 100;
    const SLOTS: usize = 16;

    fn timer() -> Timer<&'static str> {
        Timer::new(TICK, SLOTS, 32)
    }
}
