//! Selector for QNX 8 built on `ionotify()` and pulses.
//!
//! Each registered fd gets a pulse event aimed at our own channel. `ionotify()`
//! asks the fd's resource manager (e.g. `io-sock` for sockets) to send that
//! pulse once the requested conditions are met; `select` receives the pulses.
//! Arming is one-shot, so readiness is re-armed through `IoSourceState`
//! (`registered_io_source.rs`) when an operation returns `WouldBlock`.
//!
//! `ionotify()` can't report hang-ups or errors, so a zero-timeout `poll()` on
//! the fds that just pulsed reads them, before they are armed again (QNX's
//! `poll()` cancels armed notifications). A hang-up alone can't be waited for:
//! once a source is both readable and writable and nothing was read or written
//! since, it gets no further event until an operation returns `WouldBlock`.

use std::collections::HashMap;
use std::io;
use std::mem::{size_of, MaybeUninit};
use std::os::fd::RawFd;
use std::ptr;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{Interest, Token};

/// Pulse code sent by `Selector::wake`; the pulse value is the waker's token.
const WAKE: i8 = 1;
/// Pulse code of the per-fd events; the pulse value is the slot index.
const READY: i8 = 2;
/// Pulse priority: run at the priority of the thread that sends it.
const PRIO_INHERIT: libc::c_int = -1;

/// Unique id for use as `SelectorId`.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

/// Owns the channel; clones share it through the `Arc`.
#[derive(Debug)]
pub struct Selector {
    state: Arc<SelectorState>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let state = SelectorState::new()?;
        Ok(Selector {
            state: Arc::new(state),
        })
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        Ok(Selector {
            state: self.state.clone(),
        })
    }

    pub fn select(&mut self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        self.state.select(events, timeout)
    }

    cfg_io_source! {
        // Only `SourceFd` (os-ext) uses plain `register`; `net` sources use
        // `register_internal`.
        #[allow(dead_code)]
        pub fn register(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
            self.state.register(fd, token, interests, false)
        }

        pub(crate) fn register_internal(
            &self,
            fd: RawFd,
            token: Token,
            interests: Interest,
        ) -> io::Result<Arc<RegistrationRecord>> {
            self.state
                .register(fd, token, interests, true)
                .map(|()| Arc::new(RegistrationRecord::new()))
        }

        pub fn reregister(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
            self.state.reregister(fd, token, interests)
        }

        pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
            self.state.deregister(fd)
        }
    }

    pub fn wake(&self, token: Token) -> io::Result<()> {
        // The value is pointer sized, so the whole token fits.
        syscall!(MsgSendPulsePtr(
            self.state.coid,
            PRIO_INHERIT,
            WAKE.into(),
            token.0 as *mut libc::c_void,
        ))
        .map(|_| ())
    }
}

cfg_io_source! {
    impl Selector {
        #[cfg(debug_assertions)]
        pub fn id(&self) -> usize {
            self.state.id
        }
    }
}

/// The channel pulses are delivered to, and our own connection to it.
#[derive(Debug)]
struct SelectorState {
    /// The channel `select` receives pulses on.
    chid: libc::c_int,
    /// Side-channel connection to `chid`; pulse events and the Waker send through it.
    coid: libc::c_int,
    /// Read only by `Selector::id`, which exists only with `net` or `os-ext`.
    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    id: usize,
    /// Registered fds; the value of a READY pulse indexes `entries`.
    slots: Mutex<Slots>,
}

impl SelectorState {
    fn new() -> io::Result<SelectorState> {
        let chid = syscall!(ChannelCreate(libc::_NTO_CHF_PRIVATE))?;
        // nd = 0 and pid = 0 mean this node and this process. Side-channel
        // coids use their own number range, so they never collide with fds.
        let coid = match syscall!(ConnectAttach(0, 0, chid, libc::_NTO_SIDE_CHANNEL, 0)) {
            Ok(coid) => coid,
            Err(e) => {
                let _ = syscall!(ChannelDestroy(chid));
                return Err(e);
            }
        };

        Ok(SelectorState {
            chid,
            coid,
            #[cfg(debug_assertions)]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            slots: Mutex::new(Slots::default()),
        })
    }

    cfg_io_source! {
        /// Give `fd` a slot and a pulse event aimed at our channel, then arm it.
        /// Conditions already met are kept in the slot's `pending`, and a READY
        /// pulse is sent so `select` reports them.
        fn register(
            &self,
            fd: RawFd,
            token: Token,
            interests: Interest,
            internal: bool,
        ) -> io::Result<()> {
            let mut slots = self.slots.lock().unwrap();

            if slots.by_fd.contains_key(&fd) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "I/O source already registered with this `Registry` \
                    (an old file descriptor might have been closed without deregistration)",
                ));
            }

            let index = match slots.free.pop() {
                Some(index) => index,
                None => {
                    slots.entries.push(None);
                    slots.entries.len() - 1
                }
            };

            let mut event = ffi::SigEvent::pulse(self.coid, PRIO_INHERIT as i16, READY, index);
            // `fd` names the server allowed to deliver the event; the kernel rewrites
            // `event`. EAGAIN here means the RLIMIT_SIGEVENT_NP limit is reached.
            if let Err(err) = syscall!(MsgRegisterEvent(
                (&mut event as *mut ffi::SigEvent).cast(),
                fd
            )) {
                slots.free.push(index);
                return Err(err);
            }

            let pending = match arm(fd, &event, interests_to_conds(interests)) {
                Ok(met) => met,
                Err(err) => {
                    let _ = syscall!(MsgUnregisterEvent((&event as *const ffi::SigEvent).cast()));
                    slots.free.push(index);
                    return Err(err);
                }
            };

            slots.entries[index] = Some(Slot {
                fd,
                token,
                interests,
                event,
                reported: 0,
                pending,
                internal,
            });
            slots.by_fd.insert(fd, index);
            drop(slots);

            if pending != 0 {
                self.notify(index)?;
            }

            Ok(())
        }

        /// Update the slot of `fd` and arm it again. `IoSourceState::do_io` calls
        /// this on `WouldBlock`, so a condition just went false: `reported` is
        /// cleared, because a spurious event is harmless and a missed one hangs
        /// the task.
        fn reregister(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
            let mut slots = self.slots.lock().unwrap();
            let index = match slots.by_fd.get(&fd) {
                Some(&index) => index,
                None => return Err(io::ErrorKind::NotFound.into()),
            };
            let slot = slots.entries[index]
                .as_mut()
                .expect("`by_fd` only points to used slots");

            let conds = interests_to_conds(interests);
            let met = arm(fd, &slot.event, conds)?;

            slot.token = token;
            slot.interests = interests;
            slot.reported = 0;
            // Keep unreported conditions that are still wanted, add the new ones.
            slot.pending = (slot.pending & conds) | met;
            drop(slots);

            if met != 0 {
                self.notify(index)?;
            }
            Ok(())
        }

        /// Disarm `fd`, unregister its event and free its slot. A pulse already in
        /// flight for the slot is dropped by `select` if the slot is still free,
        /// or reported as a spurious event if it was reused, which mio allows.
        fn deregister(&self, fd: RawFd) -> io::Result<()> {
            let mut slots = self.slots.lock().unwrap();
            let index = match slots.by_fd.remove(&fd) {
                Some(index) => index,
                None => return Err(io::ErrorKind::NotFound.into()),
            };
            let slot = slots.entries[index]
                .take()
                .expect("`by_fd` only points to used slots");

            // POLL cancels anything still armed. The fd may already be closed or
            // its server may not support notify, so the result is ignored.
            // SAFETY: POLL takes no event.
            let _ = unsafe {
                ffi::ionotify(
                    fd,
                    ffi::NOTIFY_ACTION_POLL,
                    interests_to_conds(slot.interests),
                    std::ptr::null(),
                )
            };
            let res = syscall!(MsgUnregisterEvent(
                (&slot.event as *const ffi::SigEvent).cast()
            ));

            // Free the slot only after its event is unregistered.
            slots.free.push(index);
            slots.rearm.retain(|&i| i != index);
            res.map(|_| ())
        }
    }

    /// Wait for pulses and turn them into events. Blocks until the first one
    /// arrives or `timeout` passes, then only takes pulses already queued.
    fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();
        self.rearm_raw()?;

        // READY pulses are only collected here; `report` turns them into events.
        let mut pulsed = Vec::new();
        // A timeout too large to add to `now` waits forever.
        let deadline = timeout.and_then(|timeout| Instant::now().checked_add(timeout));
        loop {
            while events.len() + pulsed.len() < events.capacity() {
                let wait = if events.is_empty() && pulsed.is_empty() {
                    deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()))
                } else {
                    Some(Duration::ZERO)
                };
                match self.receive(wait)? {
                    Some(pulse) => self.handle(&pulse, events, &mut pulsed),
                    None => break, // timeout
                }
            }
            self.report(&pulsed, events);
            pulsed.clear();

            // A pulse for a deregistered slot, or for conditions already
            // reported, gives no event; keep waiting rather than return empty.
            let timed_out = deadline.is_some_and(|deadline| Instant::now() >= deadline);
            if !events.is_empty() || timed_out {
                return Ok(());
            }
        }
    }

    /// Receive one pulse on our channel, waiting at most `wait` (`None` waits
    /// forever). Returns `None` on timeout.
    fn receive(&self, wait: Option<Duration>) -> io::Result<Option<ffi::Pulse>> {
        if let Some(wait) = wait {
            let ns = u64::try_from(wait.as_nanos()).unwrap_or(u64::MAX);
            // Applies to the next receive-blocking call on this thread only; a
            // null event means just unblock with ETIMEDOUT.
            syscall!(TimerTimeout(
                libc::CLOCK_MONOTONIC,
                libc::_NTO_TIMEOUT_RECEIVE,
                ptr::null(),
                &ns,
                ptr::null_mut(),
            ))?;
        }

        let mut pulse = MaybeUninit::<ffi::Pulse>::uninit();
        match syscall!(MsgReceivePulse(
            self.chid,
            pulse.as_mut_ptr().cast(),
            size_of::<ffi::Pulse>(),
            ptr::null_mut()
        )) {
            // SAFETY: MsgReceivePulse succeeded, so it filled the buffer.
            Ok(_) => Ok(Some(unsafe { pulse.assume_init() })),
            Err(err) if err.raw_os_error() == Some(libc::ETIMEDOUT) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Turn the pulsed slots into events. `poll()` reads hang-ups and errors,
    /// which `ionotify()` can't report. It runs before `ready` re-arms the
    /// slots, because QNX's `poll()` cancels the fds' armed notifications.
    fn report(&self, pulsed: &[usize], events: &mut Events) {
        let slots = &mut *self.slots.lock().unwrap();
        // Deregistered slots have no fd any more; their pulse was in flight.
        let pulsed: Vec<usize> = pulsed
            .iter()
            .copied()
            .filter(|&index| matches!(slots.entries.get(index), Some(Some(_))))
            .collect();
        let mut fds: Vec<libc::pollfd> = pulsed
            .iter()
            .map(|&index| libc::pollfd {
                fd: slots.entries[index].as_ref().map_or(-1, |slot| slot.fd),
                events: libc::POLLIN | libc::POLLOUT,
                revents: 0,
            })
            .collect();
        if fds.is_empty() {
            return;
        }

        // On failure every `revents` stays 0: the events still go out, just
        // without hang-up or error flags.
        let _ = syscall!(poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 0));

        for (&index, fd) in pulsed.iter().zip(&fds) {
            Self::ready(slots, index, fd.revents, events);
        }
    }

    /// Report what is ready on slot `index`. A READY pulse doesn't say which
    /// condition fired, so `arm` is asked: it returns what is met now and
    /// arms the rest again.
    fn ready(slots: &mut Slots, index: usize, revents: libc::c_short, events: &mut Events) {
        let Some(Some(slot)) = slots.entries.get_mut(index) else {
            return; // deregistered; the pulse was already in flight
        };

        let want = interests_to_conds(slot.interests);
        // An fd closed without deregistering fails here; wake its owner, whose
        // next I/O call reports the error.
        let met = arm(slot.fd, &slot.event, want).unwrap_or(want) | slot.pending;
        slot.pending = 0;

        // Only report what isn't reported yet (edge-triggered emulation).
        let report = met & want & !slot.reported;
        if report == 0 {
            return;
        }
        slot.reported |= report;
        if !slot.internal {
            slots.rearm.push(index);
        }
        events.push(Event {
            token: slot.token,
            conds: report,
            revents,
        });
    }

    /// Re-arm the raw fds reported by the previous `select`. Nothing else
    /// re-arms them: they have no `IoSourceState::do_io`. Clearing `reported`
    /// reports a still-ready fd again, as `event_ports.rs` does.
    fn rearm_raw(&self) -> io::Result<()> {
        let slots = &mut *self.slots.lock().unwrap();
        for index in slots.rearm.drain(..) {
            let Some(slot) = slots.entries[index].as_mut() else {
                continue;
            };

            slot.reported = 0;
            let met = arm(slot.fd, &slot.event, interests_to_conds(slot.interests))?;
            if met != 0 {
                slot.pending |= met;
                self.notify(index)?;
            }
        }

        Ok(())
    }

    /// Send ourselves a READY pulse for slot `index`, so `select` looks at it.
    fn notify(&self, index: usize) -> io::Result<()> {
        // The value is pointer sized, so any slot index fits.
        syscall!(MsgSendPulsePtr(
            self.coid,
            PRIO_INHERIT,
            READY.into(),
            index as *mut libc::c_void,
        ))
        .map(|_| ())
    }

    /// Turn one pulse into at most one event.
    fn handle(&self, pulse: &ffi::Pulse, events: &mut Events, pulsed: &mut Vec<usize>) {
        // SAFETY: both of our pulse kinds are sent with a pointer-sized value.
        let value = unsafe { pulse.value.sival_ptr } as usize;
        match pulse.code {
            // Reported as readable, like the other selectors' wakers.
            WAKE => {
                events.push(Event {
                    token: Token(value),
                    conds: ffi::NOTIFY_COND_INPUT,
                    revents: 0,
                });
            }
            READY => pulsed.push(value),
            // Kernel pulses use negative codes; none are ours.
            _ => {}
        }
    }
}

impl Drop for SelectorState {
    fn drop(&mut self) {
        // Drop can't report errors; both ids were created in `new` and are still open.
        let _ = syscall!(ConnectDetach(self.coid));
        let _ = syscall!(ChannelDestroy(self.chid));
    }
}

/// Map mio interests to `ionotify()` conditions.
fn interests_to_conds(interests: Interest) -> libc::c_int {
    let mut conds = 0;
    if interests.is_readable() {
        conds |= ffi::NOTIFY_COND_INPUT;
    }
    if interests.is_writable() {
        conds |= ffi::NOTIFY_COND_OUTPUT;
    }
    conds
}

/// Arm `event` for the conditions in `conds` that aren't met yet, and return
/// the ones that are met now.
///
/// `POLLARM` arms nothing if any requested condition is already met, so the
/// met ones are removed and the rest are armed again, until one call succeeds.
/// That one call arms every remaining condition. A resource manager without
/// notify support answers `ENOSYS`; such an fd is treated as always ready.
fn arm(fd: RawFd, ev: &ffi::SigEvent, conds: libc::c_int) -> io::Result<libc::c_int> {
    let mut want = conds;
    let mut met = 0;
    while want != 0 {
        // SAFETY: `ev` was registered for `fd` with `MsgRegisterEvent` and
        // outlives the call.
        let res = unsafe { ffi::ionotify(fd, ffi::NOTIFY_ACTION_POLLARM, want, ev) };
        if res == -1 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ENOSYS) {
                return Ok(conds);
            }
            return Err(err);
        }

        let hit = want & res;
        if hit == 0 {
            break; // everything left in `want` is armed
        }
        met |= hit;
        want &= !hit;
    }
    Ok(met)
}

/// One registered fd.
#[derive(Debug)]
struct Slot {
    fd: RawFd,
    token: Token,
    interests: Interest,
    /// The event passed to `MsgRegisterEvent`. The kernel rewrites it there, so
    /// arming and unregistering must use this copy.
    event: ffi::SigEvent,
    /// Conditions already reported and still true. They aren't reported again
    /// until they go false, which emulates edge-triggered readiness.
    reported: libc::c_int,
    /// Registered through `register_internal`: `IoSourceState::do_io` re-arms it
    /// on `WouldBlock`. Other fds are re-armed by `select`.
    internal: bool,
    /// Conditions found met while arming and not reported yet. A READY pulse
    /// is sent for the slot, so `select` reports them.
    pending: libc::c_int,
}

/// The slot table.
#[derive(Debug, Default)]
struct Slots {
    /// Indexed by slot; `None` is a free slot. Slots never move, because their
    /// index is the value of an already registered pulse.
    entries: Vec<Option<Slot>>,
    /// Free slot indexes, reused before `entries` grows.
    #[cfg_attr(not(any(feature = "net", feature = "os-ext")), allow(dead_code))]
    free: Vec<usize>,
    /// fd to slot index; `reregister` and `deregister` only get the fd.
    #[cfg_attr(not(any(feature = "net", feature = "os-ext")), allow(dead_code))]
    by_fd: HashMap<RawFd, usize>,
    /// Slots that `select` must re-arm before it blocks again.
    rearm: Vec<usize>,
}

/// One readiness report: which registration, and which conditions were met.
#[derive(Debug, Clone)]
pub struct Event {
    token: Token,
    /// `NOTIFY_COND_*` bits, as returned by `ionotify()`.
    conds: libc::c_int,
    /// `POLL*` bits from a zero-timeout `poll()` after the pulse: the only
    /// public way to learn about hang-ups and errors.
    revents: libc::c_short,
}

pub type Events = Vec<Event>;

pub mod event {
    use std::fmt;

    use super::ffi;

    use crate::sys::Event;
    use crate::Token;

    pub fn token(event: &Event) -> Token {
        event.token
    }

    pub fn is_readable(event: &Event) -> bool {
        event.conds & ffi::NOTIFY_COND_INPUT != 0
    }

    pub fn is_writable(event: &Event) -> bool {
        event.conds & ffi::NOTIFY_COND_OUTPUT != 0
    }

    // `ionotify()` can't report hang-ups or errors, so these read `revents`,
    // filled in by `report`. Same mapping as the poll selector; QNX has
    // no POLLRDHUP, so a peer's half-close shows up only as readable.
    pub fn is_error(event: &Event) -> bool {
        event.revents & libc::POLLERR != 0
    }

    pub fn is_read_closed(event: &Event) -> bool {
        // Both halves of the socket have closed.
        event.revents & libc::POLLHUP != 0
    }

    pub fn is_write_closed(event: &Event) -> bool {
        // Both halves of the socket have closed.
        event.revents & libc::POLLHUP != 0
            // Unix pipe write end has closed.
            || (event.revents & libc::POLLOUT != 0 && event.revents & libc::POLLERR != 0)
            // The other side (read end) of a Unix pipe has closed.
            || event.revents == libc::POLLERR
    }

    pub fn is_priority(event: &Event) -> bool {
        event.conds & ffi::NOTIFY_COND_OBAND != 0
    }

    pub fn is_aio(_: &Event) -> bool {
        false
    }

    pub fn is_lio(_: &Event) -> bool {
        false
    }

    pub fn debug_details(f: &mut fmt::Formatter<'_>, event: &Event) -> fmt::Result {
        #[allow(clippy::trivially_copy_pass_by_ref)]
        fn check_conds(got: &libc::c_int, want: &libc::c_int) -> bool {
            got & want != 0
        }
        debug_detail!(
            CondsDetails(libc::c_int),
            check_conds,
            ffi::NOTIFY_COND_INPUT,
            ffi::NOTIFY_COND_OUTPUT,
            ffi::NOTIFY_COND_OBAND,
        );

        #[allow(clippy::trivially_copy_pass_by_ref)]
        fn check_revents(got: &libc::c_short, want: &libc::c_short) -> bool {
            got & want != 0
        }
        debug_detail!(
            ReventsDetails(libc::c_short),
            check_revents,
            libc::POLLIN,
            libc::POLLOUT,
            libc::POLLERR,
            libc::POLLHUP,
        );

        f.debug_struct("ionotify_event")
            .field("token", &event.token)
            .field("conds", &CondsDetails(event.conds))
            .field("revents", &ReventsDetails(event.revents))
            .finish()
    }
}

// The Waker sends a pulse through `Selector::wake`.
pub(crate) use crate::sys::unix::waker::Waker;

cfg_io_source! {
    mod registered_io_source;
    pub(crate) use registered_io_source::IoSourceState;
    use registered_io_source::RegistrationRecord;
}

/// QNX 8 items the libc crate doesn't provide yet.
mod ffi {
    #![allow(dead_code)]
    use libc::{c_int, c_long, c_uint, c_ulong, c_void};

    // sys/iomgr.h: conditions.
    pub(super) const NOTIFY_COND_OBAND: c_int = 0x4000_0000;
    pub(super) const NOTIFY_COND_OUTPUT: c_int = 0x2000_0000;
    pub(super) const NOTIFY_COND_INPUT: c_int = 0x1000_0000;

    // sys/iomgr.h: actions.
    pub(super) const NOTIFY_ACTION_POLL: c_int = 0x2;
    pub(super) const NOTIFY_ACTION_POLLARM: c_int = 0x3;

    // sys/siginfo.h
    pub(super) const SIGEV_PULSE: c_int = 4;

    // sys/siginfo.h: union sigval.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) union SigVal {
        pub(super) sival_int: c_int,
        pub(super) sival_ptr: *mut c_void,
        pub(super) sival_long: c_long,
        pub(super) sival_ulong: c_ulong,
    }

    // sys/siginfo.h: struct sigevent, union __sigev_un1.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) union SigevUn1 {
        pub(super) sigev_signo: c_int,
        pub(super) sigev_coid: c_int,
        pub(super) sigev_id: c_int,
        pub(super) sigev_notify_function: Option<unsafe extern "C" fn(SigVal)>,
        pub(super) sigev_addr: *mut c_uint, // volatile unsigned *
        pub(super) sigev_handle: c_uint,
    }

    // sys/siginfo.h: struct sigevent, __sigev_un2.__st.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) struct SigevSt {
        pub(super) sigev_code: i16,     // short
        pub(super) sigev_priority: i16, // short
    }

    // sys/siginfo.h: struct sigevent, union __sigev_un2.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) union SigevUn2 {
        pub(super) st: SigevSt,
        pub(super) sigev_notify_attributes: *mut libc::pthread_attr_t,
        pub(super) sigev_memop: c_int,
    }

    // sys/siginfo.h: struct sigevent.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) struct SigEvent {
        pub(super) sigev_notify: c_int,
        pub(super) sigev_un1: SigevUn1,
        pub(super) sigev_value: SigVal,
        pub(super) sigev_un2: SigevUn2,
    }

    impl std::fmt::Debug for SigEvent {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("SigEvent").finish_non_exhaustive()
        }
    }

    // SAFETY: plain data; the pointer fields are never dereferenced here.
    unsafe impl Send for SigEvent {}

    impl SigEvent {
        /// Same as the C macro SIGEV_PULSE_INIT (sys/siginfo.h).
        pub(super) fn pulse(coid: c_int, priority: i16, code: i8, value: usize) -> SigEvent {
            SigEvent {
                sigev_notify: SIGEV_PULSE,
                sigev_un1: SigevUn1 { sigev_coid: coid },
                sigev_value: SigVal {
                    sival_ptr: value as *mut c_void,
                },
                sigev_un2: SigevUn2 {
                    st: SigevSt {
                        sigev_code: code as i16,
                        sigev_priority: priority,
                    },
                },
            }
        }
    }

    // sys/neutrino.h: struct _pulse.
    #[repr(C)]
    pub(super) struct Pulse {
        pub(super) r#type: u16,
        pub(super) subtype: u16,
        pub(super) code: i8,
        pub(super) zero: [u8; 3],
        pub(super) value: SigVal,
        pub(super) scoid: i32,
    }

    // unistd.h
    extern "C" {
        pub(super) fn ionotify(
            fd: c_int,
            action: c_int,
            flags: c_int,
            event: *const SigEvent,
        ) -> c_int;
    }
}
