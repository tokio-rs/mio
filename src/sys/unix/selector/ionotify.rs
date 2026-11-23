//! Selector for QNX 8 built on `ionotify()` and pulses.
//!
//! Each registered fd gets a pulse event aimed at our own channel. `ionotify()`
//! asks the fd's resource manager (e.g. `io-sock` for sockets) to send that
//! pulse once the requested conditions are met; `select` receives the pulses.
//! Arming is one-shot, so readiness is re-armed through `IoSourceState`
//! (`registered_io_source.rs`) when an operation returns `WouldBlock`.

use std::collections::HashMap;
use std::io;
use std::os::fd::RawFd;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
        todo!()
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        todo!()
    }

    cfg_io_source! {
        pub(crate) fn register_internal(
            &self,
            fd:RawFd,
            token:Token,
            interests: Interest,
        ) -> io::Result<std::sync::Arc<RegistrationRecord>> {
            todo!()
        }
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interest) -> io::Result<()> {
        todo!()
    }

    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        todo!()
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
    /// Channel id: the mailbox `select` receives pulses on.
    chid: libc::c_int,
    /// Side-channel connection to `chid`; pulse events and the Waker send through it.
    coid: libc::c_int,
    #[cfg(debug_assertions)]
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
}

impl Drop for SelectorState {
    fn drop(&mut self) {
        // Drop can't report errors; both ids were created in `new` and are still open.
        let _ = syscall!(ConnectDetach(self.coid));
        let _ = syscall!(ChannelDestroy(self.chid));
    }
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
}

/// The slot table.
#[derive(Debug, Default)]
struct Slots {
    /// Indexed by slot; `None` is a free slot. Slots never move, because their
    /// index is the value of an already registered pulse.
    entries: Vec<Option<Slot>>,
    /// Free slot indexes, reused before `entries` grows.
    free: Vec<usize>,
    /// fd to slot index; `reregister` and `deregister` only get the fd.
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
}

pub type Events = Vec<Event>;

pub mod event {
    use std::fmt;

    use crate::sys::Event;
    use crate::Token;

    pub fn token(event: &Event) -> Token {
        todo!()
    }

    pub fn is_readable(_: &Event) -> bool {
        todo!()
    }

    pub fn is_writable(_: &Event) -> bool {
        todo!()
    }

    pub fn is_error(_: &Event) -> bool {
        todo!()
    }

    pub fn is_read_closed(_: &Event) -> bool {
        todo!()
    }

    pub fn is_write_closed(_: &Event) -> bool {
        todo!()
    }

    pub fn is_priority(_: &Event) -> bool {
        todo!()
    }

    pub fn is_aio(_: &Event) -> bool {
        false
    }

    pub fn is_lio(_: &Event) -> bool {
        false
    }

    pub fn debug_details(f: &mut fmt::Formatter<'_>, event: &Event) -> fmt::Result {
        fmt::Debug::fmt(event, f)
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
/// Headers: <SDP>/target/qnx/usr/include/
mod ffi {
    #![allow(dead_code)]
    use libc::{c_int, c_long, c_uint, c_ulong, c_void};

    // ---- sys/iomgr.h  conditions (the "what") ----
    pub(super) const NOTIFY_COND_OBAND: c_int = 0x4000_0000;
    pub(super) const NOTIFY_COND_OUTPUT: c_int = 0x2000_0000;
    pub(super) const NOTIFY_COND_INPUT: c_int = 0x1000_0000;

    // ---- sys/iomgr.h  actions (the "how") ----
    pub(super) const NOTIFY_ACTION_POLL: c_int = 0x2;
    pub(super) const NOTIFY_ACTION_POLLARM: c_int = 0x3;

    // ---- sys/siginfo.h ----
    pub(super) const SIGEV_PULSE: c_int = 4;

    // ---- sys/siginfo.h  union sigval ----
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) union SigVal {
        pub(super) sival_int: c_int,
        pub(super) sival_ptr: *mut c_void,
        pub(super) sival_long: c_long,
        pub(super) sival_ulong: c_ulong,
    }

    // ---- sys/siginfo.h  struct sigevent: union __sigev_un1 ----
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

    // ---- sys/siginfo.h  struct sigevent: __sigev_un2.__st ----
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) struct SigevSt {
        pub(super) sigev_code: i16,     // short
        pub(super) sigev_priority: i16, // short
    }

    // ---- sys/siginfo.h  struct sigevent: union __sigev_un2 ----
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) union SigevUn2 {
        pub(super) st: SigevSt,
        pub(super) sigev_notify_attributes: *mut libc::pthread_attr_t,
        pub(super) sigev_memop: c_int,
    }

    // ---- sys/siginfo.h  struct sigevent ----
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

    // ---- sys/neutrino.h  struct _pulse ----
    #[repr(C)]
    pub(super) struct Pulse {
        pub(super) r#type: u16,
        pub(super) subtype: u16,
        pub(super) code: i8,
        pub(super) zero: [u8; 3],
        pub(super) value: SigVal,
        pub(super) scoid: i32,
    }

    // ---- unistd.h ----
    extern "C" {
        pub(super) fn ionotify(
            fd: c_int,
            action: c_int,
            flags: c_int,
            event: *const SigEvent,
        ) -> c_int;
    }
}
